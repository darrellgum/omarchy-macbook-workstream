# SPDX-License-Identifier: CC0-1.0
CC ?= cc
CFLAGS ?= -O2
WARNINGS = -std=c17 -Wall -Wextra -Wpedantic -Werror
BUILD = .build

.PHONY: all check test sanitize clean

all: $(BUILD)/t1-camera-relay

$(BUILD):
	mkdir -p -- "$@"

$(BUILD)/t1-camera-relay: relay.c | $(BUILD)
	$(CC) $(CPPFLAGS) $(CFLAGS) $(WARNINGS) $< $(LDFLAGS) $(LDLIBS) -o $@

$(BUILD)/relay-test: tests/relay_test.c relay.c | $(BUILD)
	$(CC) $(CPPFLAGS) $(CFLAGS) $(WARNINGS) -UNDEBUG $< $(LDFLAGS) $(LDLIBS) -o $@

$(BUILD)/relay-test-sanitize: tests/relay_test.c relay.c | $(BUILD)
	$(CC) $(CPPFLAGS) -O1 -g $(WARNINGS) -UNDEBUG -fno-omit-frame-pointer \
		-fsanitize=address,undefined $< $(LDFLAGS) $(LDLIBS) -o $@

test: $(BUILD)/relay-test
	./$(BUILD)/relay-test

sanitize: $(BUILD)/relay-test-sanitize
	ASAN_OPTIONS=detect_leaks=1:halt_on_error=1 UBSAN_OPTIONS=halt_on_error=1 \
		./$(BUILD)/relay-test-sanitize

check: test sanitize
	@for script in t1-camera-start t1-camera; do bash -n "$$script" || exit; done

# Remove only this Makefile's three generated executables.
clean:
	@for file in $(BUILD)/t1-camera-relay $(BUILD)/relay-test $(BUILD)/relay-test-sanitize; do \
		if [ -f "$$file" ]; then rm -- "$$file"; fi; \
	done
