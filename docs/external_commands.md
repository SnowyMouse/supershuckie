# Super Shuckie external commands

You can control Super Shuckie using REST requests. To enable this feature, you
will need the feature enabled inside the application itself.

You can then make the following GET requests to `127.0.0.1:30158`.

## Table of contents

- [increment-counter](#increment-counter)
- [mark-start](#mark-start)
- [mark-end](#mark-end)
- [stats](#stats)

## increment-counter

Increment a counter. A replay must be recording for this to work.

Usage:

- `http://127.0.0.1:30158/increment-counter?name=NAME`
- `http://127.0.0.1:30158/increment-counter?name=NAME&by=BY`

Arguments:

| Argument | Default    | Description                                                  |
|----------|------------|--------------------------------------------------------------|
| `name`   | (required) | The name of the counter.                                     |
| `by`     | 1          | Amount to increment (or decrement, if negative) the counter. |

## mark-start

Mark the start of a replay and enables the timer feature. A replay must be
recording for this to work.

Usage:

- `http://127.0.0.1:30158/mark-start`
- `http://127.0.0.1:30158/mark-start?offset=OFFSET`

Arguments:

| Argument | Default | Description                 |
|----------|---------|-----------------------------|
| `offset` | 0       | The offset in milliseconds. |

## mark-end

Mark the start of a replay and enables the timer feature. A replay must be
recording for this to work.

Usage:

- `http://127.0.0.1:30158/mark-end`

## stats

Get the current stats in JSON format. All timestamps are in milliseconds and are
unsigned (non-negative) unless otherwise specified.

Usage:

- `http://127.0.0.1:30158/stats`

| Field                  | Type                     | Description                                                                                                         |
|------------------------|--------------------------|---------------------------------------------------------------------------------------------------------------------|
| `time_start`           | `number \| null`         | Time when the timer starts. This is the minimum value of `time_current` before `time_offset` is added.              |
| `time_end`             | `number \| null`         | Time when the timer ends. This is the maximum value of `time_current` before `time_offset` is added.                |
| `time_offset`          | `number \| null`         | Time to add to the timer AFTER clamping between `time_start` and `time_end`.                                        |
| `time_current`         | `number \| null`         | Current timer value. This has `time_offset` pre-added to it, and it is clamped between `time_start` and `time_end`. |
| `total_elapsed_time`   | `number`                 | Total time the core has been running. If in a replay, this is the elapsed time of the replay, instead.              |
| `total_elapsed_frames` | `number`                 | Total number of frames the core has been running. If in a replay, this is the frame counter of the replay, instead. |
| `is_recording`         | `boolean`                | `true` if currently recording a replay, `false` if not.                                                             |
| `is_playing_back`      | `boolean`                | `true` if currently playing back a replay, `false` if not.                                                          |
| `is_paused`            | `boolean`                | `true` if the user has manually paused, `false` if not.                                                             |
| `current_speed`        | `number`                 | The current playback speed multiplier.                                                                              |
| `counters`             | `record<string, number>` | The current values of all counters in the currently playing/recording replay.                                       |
