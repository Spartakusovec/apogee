#!/bin/bash
# Runs the given command (lock, screen off) only when no sound is playing,
# so the screen doesn't lock while you watch a video.
if pactl list short sinks 2>/dev/null | grep -q RUNNING; then
    exit 0
fi
exec "$@"
