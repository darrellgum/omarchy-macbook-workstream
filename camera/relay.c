/* SPDX-License-Identifier: CC0-1.0 */
/* On-demand T1 H.264 camera relay. No image files, audio, shell or network.
 * The client-usage event and controls below are the v4l2loopback 0.15.4 ABI.
 * ABI reference: https://github.com/umlaeute/v4l2loopback/blob/v0.15.4/v4l2loopback.c
 * Build: cc -O2 -std=c17 -Wall -Wextra -Wpedantic -Werror relay.c -o relay
 */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <linux/videodev2.h>
#include <poll.h>
#include <signal.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/prctl.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

enum { WIDTH = 1280, HEIGHT = 720, FPS = 30, MAX_STARTS = 3,
       CLEAR_FRAMES = 32 };
#define FRAME_BYTES ((size_t)WIDTH * HEIGHT * 3 / 2)
#define CLIENT_USAGE_EVENT (V4L2_EVENT_PRIVATE_START + 0x08e00000U + 1U)
#define KEEP_FORMAT ((V4L2_CID_USER_BASE | 0xf000U) + 0U)
#define SUSTAIN_FRAMERATE ((V4L2_CID_USER_BASE | 0xf000U) + 1U)

static volatile sig_atomic_t stopping;

struct decoder {
    pid_t pid;
    int pipe_fd;
    size_t filled;
    unsigned starts;
    uint64_t started_ms;
    uint64_t frame_ms;
    uint64_t retry_ms;
    bool got_frame;
    int exit_status;
    bool status_known;
};

static void signal_stop(int signal_number)
{
    (void)signal_number;
    stopping = 1;
}

static uint64_t monotonic_ms(void)
{
    struct timespec now;
    if (clock_gettime(CLOCK_MONOTONIC, &now) < 0)
        return 0;
    return (uint64_t)now.tv_sec * 1000U + (uint64_t)now.tv_nsec / 1000000U;
}

static bool deadline_exceeded(uint64_t now, uint64_t since, uint64_t duration)
{
    /* A timestamp taken before fork can precede the child's recorded start.
     * Guard subtraction so unsigned wrap cannot kill a just-started decoder. */
    return now >= since && now - since > duration;
}

static int checked_ioctl(int fd, unsigned long request, void *argument)
{
    int result;
    do {
        result = ioctl(fd, request, argument);
    } while (result < 0 && errno == EINTR && !stopping);
    return result;
}

static void error_message(const char *operation)
{
    fprintf(stderr, "t1-camera: %s: %s\n", operation, strerror(errno));
}

static int open_video(const char *path, struct stat *metadata)
{
    if (path[0] != '/') {
        errno = EINVAL;
        return -1;
    }
    int fd = open(path, O_RDWR | O_NONBLOCK | O_CLOEXEC);
    if (fd < 0)
        return -1;
    if (fstat(fd, metadata) < 0 || !S_ISCHR(metadata->st_mode)) {
        close(fd);
        errno = ENODEV;
        return -1;
    }
    return fd;
}

static uint32_t device_caps(const struct v4l2_capability *caps)
{
    return (caps->capabilities & V4L2_CAP_DEVICE_CAPS)
        ? caps->device_caps : caps->capabilities;
}

/* Queries and TRY_FMT do not start physical capture. */
static int validate_source(const char *path, dev_t *device)
{
    struct stat metadata;
    int fd = open_video(path, &metadata);
    if (fd < 0)
        return -1;
    struct v4l2_capability caps = {0};
    int result = -1;
    if (checked_ioctl(fd, VIDIOC_QUERYCAP, &caps) < 0)
        goto done;
    if ((device_caps(&caps) & (V4L2_CAP_VIDEO_CAPTURE | V4L2_CAP_STREAMING))
        != (V4L2_CAP_VIDEO_CAPTURE | V4L2_CAP_STREAMING)) {
        errno = ENOTSUP;
        goto done;
    }
    struct v4l2_format format = { .type = V4L2_BUF_TYPE_VIDEO_CAPTURE };
    format.fmt.pix.width = WIDTH;
    format.fmt.pix.height = HEIGHT;
    format.fmt.pix.pixelformat = V4L2_PIX_FMT_H264;
    format.fmt.pix.field = V4L2_FIELD_ANY;
    if (checked_ioctl(fd, VIDIOC_TRY_FMT, &format) < 0)
        goto done;
    if (format.fmt.pix.width != WIDTH || format.fmt.pix.height != HEIGHT
        || format.fmt.pix.pixelformat != V4L2_PIX_FMT_H264) {
        errno = ENOTSUP;
        goto done;
    }
    *device = metadata.st_rdev;
    result = 0;
done:
    close(fd);
    return result;
}

static int control(int fd, uint32_t id, int value)
{
    struct v4l2_control setting = { .id = id, .value = value };
    return checked_ioctl(fd, VIDIOC_S_CTRL, &setting);
}

static int configure_sink(int fd)
{
    struct v4l2_capability caps = {0};
    if (checked_ioctl(fd, VIDIOC_QUERYCAP, &caps) < 0)
        return -1;
    /* A previously locked exclusive-caps sink can report CAPTURE even before
     * this writer attaches. Require the exact loopback driver in either case. */
    if (strncmp((const char *)caps.driver, "v4l2 loopback", sizeof(caps.driver))
        || !(device_caps(&caps) & V4L2_CAP_READWRITE)) {
        errno = ENOTSUP;
        return -1;
    }
    struct v4l2_format format = { .type = V4L2_BUF_TYPE_VIDEO_OUTPUT };
    format.fmt.pix.width = WIDTH;
    format.fmt.pix.height = HEIGHT;
    format.fmt.pix.pixelformat = V4L2_PIX_FMT_YUV420;
    format.fmt.pix.field = V4L2_FIELD_NONE;
    format.fmt.pix.bytesperline = WIDTH;
    format.fmt.pix.sizeimage = (uint32_t)FRAME_BYTES;
    format.fmt.pix.colorspace = V4L2_COLORSPACE_REC709;
    format.fmt.pix.ycbcr_enc = V4L2_YCBCR_ENC_709;
    format.fmt.pix.quantization = V4L2_QUANTIZATION_LIM_RANGE;
    if (checked_ioctl(fd, VIDIOC_S_FMT, &format) < 0)
        return -1;
    if (format.fmt.pix.width != WIDTH || format.fmt.pix.height != HEIGHT
        || format.fmt.pix.pixelformat != V4L2_PIX_FMT_YUV420
        || format.fmt.pix.bytesperline != WIDTH
        || format.fmt.pix.sizeimage != FRAME_BYTES) {
        errno = ENOTSUP;
        return -1;
    }
    struct v4l2_streamparm rate = { .type = V4L2_BUF_TYPE_VIDEO_OUTPUT };
    rate.parm.output.timeperframe.numerator = 1;
    rate.parm.output.timeperframe.denominator = FPS;
    if (checked_ioctl(fd, VIDIOC_S_PARM, &rate) < 0)
        return -1;
    if (rate.parm.output.timeperframe.numerator != 1
        || rate.parm.output.timeperframe.denominator != FPS) {
        errno = ENOTSUP;
        return -1;
    }
    if (control(fd, KEEP_FORMAT, 1) < 0
        || control(fd, SUSTAIN_FRAMERATE, 0) < 0)
        return -1;
    struct v4l2_event_subscription subscription = {
        .type = CLIENT_USAGE_EVENT,
        .flags = V4L2_EVENT_SUB_FL_SEND_INITIAL,
    };
    return checked_ioctl(fd, VIDIOC_SUBSCRIBE_EVENT, &subscription);
}

/* A loopback write is one complete frame, not a stream to retry in pieces. */
static int write_frame(int fd, const unsigned char *frame)
{
    ssize_t count;
    do {
        count = write(fd, frame, FRAME_BYTES);
    } while (count < 0 && errno == EINTR && !stopping);
    if (count == (ssize_t)FRAME_BYTES)
        return 0;
    if (count >= 0)
        errno = EIO;
    return -1;
}

static int clear_frames(int fd, unsigned char *frame)
{
    memset(frame, 16, (size_t)WIDTH * HEIGHT);
    memset(frame + (size_t)WIDTH * HEIGHT, 128, FRAME_BYTES / 3);
    /* 0.15.4 permits at most 32 ring buffers. Clear every retained image, not
     * merely the newest slot, before any future capture consumer arrives. */
    for (unsigned i = 0; i < CLEAR_FRAMES; ++i)
        if (write_frame(fd, frame) < 0)
            return -1;
    return 0;
}

static int client_state(int fd, bool *wanted)
{
    for (;;) {
        struct v4l2_event event = {0};
        if (checked_ioctl(fd, VIDIOC_DQEVENT, &event) < 0) {
            /* Linux v4l2-event.c returns ENOENT for an empty nonblocking
             * event queue; unlike frame dequeue, this is not just EAGAIN. */
            return errno == ENOENT || errno == EAGAIN ? 0 : -1;
        }
        if (event.type == CLIENT_USAGE_EVENT) {
            uint32_t streaming;
            memcpy(&streaming, event.u.data, sizeof(streaming));
            if (streaming > 1) {
                errno = EPROTO;
                return -1;
            }
            *wanted = streaming != 0;
        }
    }
}

static void close_pipe(struct decoder *decoder)
{
    if (decoder->pipe_fd >= 0)
        close(decoder->pipe_fd);
    decoder->pipe_fd = -1;
    decoder->filled = 0;
}

/* Never let an unresponsive decoder hang service shutdown indefinitely. */
static int stop_decoder(struct decoder *decoder)
{
    close_pipe(decoder);
    if (decoder->pid <= 0)
        return 0;
    (void)kill(decoder->pid, SIGTERM);
    for (unsigned phase = 0; phase < 2; ++phase) {
        uint64_t deadline = monotonic_ms() + (phase == 0 ? 1500U : 1000U);
        do {
            int status;
            pid_t result = waitpid(decoder->pid, &status, WNOHANG);
            if (result == decoder->pid || (result < 0 && errno == ECHILD)) {
                if (result == decoder->pid) {
                    decoder->exit_status = status;
                    decoder->status_known = true;
                }
                decoder->pid = 0;
                return 0;
            }
            if (result < 0 && errno != EINTR)
                return -1;
            struct timespec delay = { .tv_sec = 0, .tv_nsec = 20000000 };
            (void)nanosleep(&delay, NULL);
        } while (monotonic_ms() < deadline);
        (void)kill(decoder->pid, SIGKILL);
    }
    errno = ETIMEDOUT;
    return -1;
}

static int start_decoder(struct decoder *decoder, const char *source)
{
    int pipe_fds[2];
    if (pipe2(pipe_fds, O_CLOEXEC) < 0)
        return -1;
    int flags = fcntl(pipe_fds[0], F_GETFL);
    if (flags < 0 || fcntl(pipe_fds[0], F_SETFL, flags | O_NONBLOCK) < 0) {
        close(pipe_fds[0]);
        close(pipe_fds[1]);
        return -1;
    }
    pid_t parent = getpid();
    pid_t child = fork();
    if (child == 0) {
        if (prctl(PR_SET_PDEATHSIG, SIGKILL) < 0 || getppid() != parent)
            _exit(126);
        struct sigaction defaults = { .sa_handler = SIG_DFL };
        sigemptyset(&defaults.sa_mask);
        (void)sigaction(SIGTERM, &defaults, NULL);
        (void)sigaction(SIGINT, &defaults, NULL);
        (void)sigaction(SIGHUP, &defaults, NULL);
        int null_fd = open("/dev/null", O_RDWR | O_CLOEXEC);
        if (null_fd < 0 || dup2(null_fd, STDIN_FILENO) < 0
            || dup2(null_fd, STDERR_FILENO) < 0
            || dup2(pipe_fds[1], STDOUT_FILENO) < 0)
            _exit(126);
        close(null_fd);
        close(pipe_fds[0]);
        close(pipe_fds[1]);
        (void)unsetenv("FFREPORT");
        execl("/usr/bin/ffmpeg", "ffmpeg", "-hide_banner", "-loglevel", "error",
              "-nostdin", "-threads", "1", "-filter_threads", "1",
              "-f", "v4l2", "-input_format", "h264",
              "-video_size", "1280x720", "-framerate", "30", "-i", source,
              "-an", "-sn", "-dn", "-vf", "scale=1280:720:out_range=tv",
              "-pix_fmt", "yuv420p", "-threads", "1", "-fps_mode", "passthrough",
              "-f", "rawvideo", "pipe:1", (char *)NULL);
        _exit(127);
    }
    close(pipe_fds[1]);
    if (child < 0) {
        close(pipe_fds[0]);
        return -1;
    }
    decoder->pid = child;
    decoder->pipe_fd = pipe_fds[0];
    decoder->filled = 0;
    decoder->got_frame = false;
    decoder->status_known = false;
    decoder->started_ms = monotonic_ms();
    decoder->frame_ms = decoder->started_ms;
    ++decoder->starts;
    return 0;
}

static int retry_decoder(struct decoder *decoder, int sink, unsigned char *frame,
                         const char *reason)
{
    if (stop_decoder(decoder) < 0 || clear_frames(sink, frame) < 0)
        return -1;
    decoder->retry_ms = monotonic_ms() + 500U * decoder->starts;
    const char *retry = decoder->starts < MAX_STARTS
        ? "bounded retry pending" : "retry limit reached; reopen the camera to retry";
    if (decoder->status_known && WIFEXITED(decoder->exit_status))
        fprintf(stderr, "t1-camera: %s; decoder exit=%d; %s\n", reason,
                WEXITSTATUS(decoder->exit_status), retry);
    else if (decoder->status_known && WIFSIGNALED(decoder->exit_status))
        fprintf(stderr, "t1-camera: %s; decoder signal=%d; %s\n", reason,
                WTERMSIG(decoder->exit_status), retry);
    else
        fprintf(stderr, "t1-camera: %s; decoder status unavailable; %s\n", reason, retry);
    return 0;
}

/* At most one complete frame per iteration, so STREAMOFF takes priority even
 * when the decoder continually fills its pipe. */
static int read_frame(struct decoder *decoder, int sink, unsigned char *frame)
{
    while (decoder->filled < FRAME_BYTES) {
        ssize_t count = read(decoder->pipe_fd, frame + decoder->filled,
                             FRAME_BYTES - decoder->filled);
        if (count > 0) {
            decoder->filled += (size_t)count;
            continue;
        }
        if (count < 0 && (errno == EAGAIN || errno == EINTR))
            return 0;
        return 1;
    }
    if (write_frame(sink, frame) < 0)
        return -1;
    decoder->filled = 0;
    decoder->got_frame = true;
    decoder->frame_ms = monotonic_ms();
    return 0;
}

static int relay_loop(int sink, const char *source, unsigned char *frame,
                      unsigned char *black)
{
    struct decoder decoder = { .pipe_fd = -1 };
    bool wanted = false;
    bool previous = false;
    int result = 0;
    uint64_t black_ms = monotonic_ms();
    const char *failure = "relay stopped";
    fprintf(stderr, "t1-camera: ready; physical capture starts only for a streaming consumer\n");
    while (!stopping) {
        if (client_state(sink, &wanted) < 0) {
            failure = "reading loopback consumer events";
            result = -1;
            break;
        }
        if (!wanted && previous) {
            if (stop_decoder(&decoder) < 0 || clear_frames(sink, black) < 0) {
                result = -1;
                break;
            }
            memset(frame, 0, FRAME_BYTES);
            black_ms = monotonic_ms();
            fprintf(stderr, "t1-camera: idle; physical capture stopped\n");
        }
        if (wanted && !previous) {
            if (clear_frames(sink, black) < 0) {
                result = -1;
                break;
            }
            black_ms = monotonic_ms();
            decoder.starts = 0;
            decoder.retry_ms = 0;
        }
        previous = wanted;
        uint64_t now = monotonic_ms();
        if (wanted && decoder.pid == 0 && decoder.starts < MAX_STARTS
            && now >= decoder.retry_ms) {
            if (start_decoder(&decoder, source) < 0) {
                result = -1;
                break;
            }
            now = monotonic_ms();
        }
        if (decoder.pid > 0) {
            int status;
            pid_t child = waitpid(decoder.pid, &status, WNOHANG);
            if (child == decoder.pid) {
                decoder.exit_status = status;
                decoder.status_known = true;
                decoder.pid = 0;
                if (retry_decoder(&decoder, sink, black, "decoder exited") < 0) {
                    result = -1;
                    break;
                }
            } else if (child < 0 && errno != EINTR) {
                result = -1;
                break;
            } else if ((!decoder.got_frame && deadline_exceeded(now, decoder.started_ms, 10000U))
                       || (decoder.got_frame && deadline_exceeded(now, decoder.frame_ms, 5000U))) {
                const char *reason = decoder.got_frame ? "frame stall deadline" : "startup deadline";
                if (retry_decoder(&decoder, sink, black, reason) < 0) {
                    result = -1;
                    break;
                }
            }
        }
        /* Loopback retains capture timestamps along with pixels. Keep idle
         * black timestamps fresh at the existing 250ms polling cadence, so
         * reopening does not produce a minutes-long jump after its first
         * cached frame. The separate black buffer never overwrites a partial
         * decoded frame during startup. This does not open physical capture. */
        if ((!wanted || decoder.pid == 0 || !decoder.got_frame)
            && now >= black_ms && now - black_ms >= 250U) {
            if (write_frame(sink, black) < 0) {
                failure = "refreshing idle black frame";
                result = -1;
                break;
            }
            black_ms = now;
        }
        struct pollfd fds[2] = {
            { .fd = sink, .events = POLLPRI },
            { .fd = decoder.pipe_fd, .events = POLLIN },
        };
        int count = poll(fds, 2, 250);
        if (count < 0) {
            if (errno == EINTR)
                continue;
            result = -1;
            break;
        }
        if (fds[0].revents & (POLLERR | POLLHUP | POLLNVAL)) {
            errno = ENODEV;
            result = -1;
            break;
        }
        if (stopping || (fds[0].revents & POLLPRI))
            continue;
        if (wanted && decoder.pipe_fd >= 0
            && (fds[1].revents & (POLLIN | POLLHUP | POLLERR | POLLNVAL))) {
            int read_result = read_frame(&decoder, sink, frame);
            if (read_result < 0
                || (read_result > 0
                    && retry_decoder(&decoder, sink, black, "decoder pipe ended") < 0)) {
                result = -1;
                break;
            }
        }
    }
    if (result < 0)
        error_message(failure);
    if (stop_decoder(&decoder) < 0) {
        error_message("decoder cleanup");
        result = -1;
    }
    if (clear_frames(sink, black) < 0) {
        error_message("clearing output");
        result = -1;
    }
    return result;
}

int main(int argc, char **argv)
{
    if (argc != 3 || getuid() == 0 || geteuid() == 0) {
        fprintf(stderr, "Usage (as desktop user): %s SOURCE_VIDEO SINK_LOOPBACK\n", argv[0]);
        return EXIT_FAILURE;
    }
    struct sigaction action = { .sa_handler = signal_stop };
    sigemptyset(&action.sa_mask);
    if (sigaction(SIGINT, &action, NULL) < 0
        || sigaction(SIGTERM, &action, NULL) < 0
        || sigaction(SIGHUP, &action, NULL) < 0) {
        error_message("signal setup");
        return EXIT_FAILURE;
    }
    dev_t source_device;
    if (validate_source(argv[1], &source_device) < 0) {
        error_message("source must offer H264 at 1280x720");
        return EXIT_FAILURE;
    }
    struct stat metadata;
    int sink = open_video(argv[2], &metadata);
    if (sink < 0) {
        error_message("open loopback");
        return EXIT_FAILURE;
    }
    if (metadata.st_rdev == source_device || configure_sink(sink) < 0) {
        fprintf(stderr, "t1-camera: sink must be a separate v4l2loopback device accepting 1280x720 YU12/30\n");
        close(sink);
        return EXIT_FAILURE;
    }
    unsigned char *frame = malloc(FRAME_BYTES);
    unsigned char *black = malloc(FRAME_BYTES);
    if (!frame || !black) {
        free(frame);
        free(black);
        close(sink);
        return EXIT_FAILURE;
    }
    int result = clear_frames(sink, black);
    if (result == 0 && !stopping)
        result = relay_loop(sink, argv[1], frame, black);
    else if (result < 0)
        error_message("initial black frames");
    memset(frame, 0, FRAME_BYTES);
    free(frame);
    free(black);
    close(sink);
    return result == 0 ? EXIT_SUCCESS : EXIT_FAILURE;
}
