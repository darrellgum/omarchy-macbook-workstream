/* SPDX-License-Identifier: CC0-1.0 */
/* Synthetic-only regression checks: no camera, network, or image files.
 * From the camera directory:
 * cc -O1 -g -std=c17 -Wall -Wextra -Wpedantic -Werror \
 *   -fsanitize=address,undefined tests/relay_test.c -o /tmp/t1-relay-test
 * /tmp/t1-relay-test
 */
#define ioctl mock_ioctl
#define main relay_entry
#include "../relay.c"
#undef main
#undef ioctl
#include <assert.h>
#include <stdarg.h>

static int remaining_events;
static uint32_t event_streaming;
static int empty_errno;

int mock_ioctl(int fd, unsigned long request, ...)
{
    (void)fd;
    assert(request == VIDIOC_DQEVENT);
    if (!remaining_events) {
        errno = empty_errno;
        return -1;
    }
    --remaining_events;
    va_list arguments;
    va_start(arguments, request);
    struct v4l2_event *event = va_arg(arguments, struct v4l2_event *);
    va_end(arguments);
    event->type = CLIENT_USAGE_EVENT;
    memcpy(event->u.data, &event_streaming, sizeof(event_streaming));
    return 0;
}

static void event_queue_tests(void)
{
    bool wanted = false;
    empty_errno = ENOENT;
    assert(client_state(-1, &wanted) == 0 && !wanted);
    remaining_events = 1;
    event_streaming = 1;
    assert(client_state(-1, &wanted) == 0 && wanted);
    assert(client_state(-1, &wanted) == 0 && wanted);
    remaining_events = 1;
    event_streaming = 0;
    assert(client_state(-1, &wanted) == 0 && !wanted);
    empty_errno = EAGAIN;
    assert(client_state(-1, &wanted) == 0 && !wanted);
    empty_errno = ENODEV;
    assert(client_state(-1, &wanted) == -1 && errno == ENODEV);
    remaining_events = 1;
    event_streaming = 2;
    assert(client_state(-1, &wanted) == -1 && errno == EPROTO);
}

static void frame_tests(void)
{
    unsigned char *frame = calloc(1, FRAME_BYTES);
    unsigned char *black = malloc(FRAME_BYTES);
    assert(frame && black);
    int sink = open("/dev/null", O_WRONLY | O_CLOEXEC);
    assert(sink >= 0 && clear_frames(sink, black) == 0);
    for (size_t i = 0; i < FRAME_BYTES; ++i)
        assert(black[i] == (i < (size_t)WIDTH * HEIGHT ? 16 : 128));
    int pipe_fds[2];
    assert(pipe2(pipe_fds, O_CLOEXEC | O_NONBLOCK) == 0);
    struct decoder decoder = { .pipe_fd = pipe_fds[0] };
    assert(write(pipe_fds[1], "abcdef", 6) == 6);
    assert(read_frame(&decoder, sink, frame) == 0);
    assert(decoder.filled == 6 && !memcmp(frame, "abcdef", 6));
    /* Idle/warmup heartbeats cannot corrupt an in-progress decoded frame. */
    assert(write_frame(sink, black) == 0);
    assert(clear_frames(sink, black) == 0);
    assert(decoder.filled == 6 && !memcmp(frame, "abcdef", 6));
    decoder.filled = FRAME_BYTES - 6;
    assert(write(pipe_fds[1], "uvwxyz", 6) == 6);
    assert(read_frame(&decoder, sink, frame) == 0);
    assert(decoder.filled == 0 && decoder.got_frame);
    close(pipe_fds[1]);
    assert(read_frame(&decoder, sink, frame) == 1);
    close_pipe(&decoder);
    assert(decoder.pipe_fd == -1 && decoder.filled == 0);
    close(sink);
    free(frame);
    free(black);
}

static void deadline_tests(void)
{
    assert(!deadline_exceeded(100, 101, 10000));
    assert(!deadline_exceeded(101, 101, 10000));
    assert(!deadline_exceeded(10101, 101, 10000));
    assert(deadline_exceeded(10102, 101, 10000));
    assert(!deadline_exceeded(0, UINT64_MAX, 5000));
    assert(deadline_exceeded(6000, 1, 5000));
    struct stat metadata;
    assert(open_video("relative", &metadata) < 0 && errno == EINVAL);
}

static void child_cleanup_tests(void)
{
    int ready[2];
    assert(pipe(ready) == 0);
    pid_t child = fork();
    assert(child >= 0);
    if (child == 0) {
        close(ready[0]);
        signal(SIGTERM, SIG_IGN);
        assert(write(ready[1], "x", 1) == 1);
        close(ready[1]);
        for (;;)
            pause();
    }
    close(ready[1]);
    char byte;
    assert(read(ready[0], &byte, 1) == 1);
    close(ready[0]);
    struct decoder decoder = { .pid = child, .pipe_fd = -1 };
    uint64_t started = monotonic_ms();
    assert(stop_decoder(&decoder) == 0);
    assert(decoder.pid == 0 && decoder.status_known);
    assert(monotonic_ms() - started < 4000);
    assert(WIFSIGNALED(decoder.exit_status));
    assert(WTERMSIG(decoder.exit_status) == SIGKILL);
    assert(waitpid(child, NULL, WNOHANG) == -1 && errno == ECHILD);

    child = fork();
    assert(child >= 0);
    if (child == 0)
        _exit(7);
    siginfo_t status;
    assert(waitid(P_PID, (id_t)child, &status, WEXITED | WNOWAIT) == 0);
    decoder.pid = child;
    decoder.status_known = false;
    assert(stop_decoder(&decoder) == 0 && decoder.status_known);
    assert(WIFEXITED(decoder.exit_status));
    assert(WEXITSTATUS(decoder.exit_status) == 7);
}

int main(void)
{
    event_queue_tests();
    frame_tests();
    deadline_tests();
    child_cleanup_tests();
    puts("PASS: event queues, partial frames, separate black frames, watchdog ordering, child cleanup/status");
    return 0;
}
