# Super Shuckie external commands

You can control Super Shuckie using REST requests. To enable this feature, you
will need the feature enabled inside the application itself.

The server is `127.0.0.1:30158`

## Table of contents

- [JS API](#js-api)
  - [Usage](#usage)
    - [JavaScript](#javascript)
    - [TypeScript](#typescript)
  - [Example code](#example-code)
- [Rest command reference](#rest-command-reference)
  - [enumerate-replays](#enumerate-replays)
  - [go-to-frame](#go-to-frame)
  - [increment-counter](#increment-counter)
  - [load-replay](#load-replay)
  - [mark-start](#mark-start)
  - [mark-end](#mark-end)
  - [set-paused](#set-paused)
  - [set-playback-speed](#set-playback-speed)
  - [stats](#stats)

## JS API

To make things simple, you can use the JavaScript API. You can use it from
Super Shuckie's source code (`/supershuckie-frontend-webserver/js/client.js`) or
load it from a Super Shuckie instance at `http://127.0.0.1:30158/client.js`

### Usage

First, you will need to somehow import it. There are a few ways you can do this.

#### JavaScript

You can import it in a JavaScript module by putting this at the top of your
module:

```javascript
import { SuperShuckieClient } from "http://127.0.0.1:30158/client.js"
```

You can alternatively import it dynamically from within an async function with
this:

```javascript
const { SuperShuckieClient } = await import("http://127.0.0.1:30158/client.js")
```

#### TypeScript

If you are using TypeScript, you can directly use the definitions and code from
Super Shuckie's source tree at `/supershuckie-frontend-webserver/js`

Ensure that client.d.ts and client.js are in the same directory. Then import it
like this:

```typescript
import { SuperShuckieClient } from "./somepath/client"
```

### Example code

```html
<script type="module">
    // Import the client...
    import { SuperShuckieClient } from "http://127.0.0.1:30158/client.js"
    
    // Next, create the client.
    const shuckie = new SuperShuckieClient();

    // Here's a function updating your overlay
    async function update() {
        const stats = await shuckie.stats()

        // Start the timer (at a 1 second offset) if it has not already been set
        if (stats.is_recording && stats.time_current == null) {
            await shuckie.mark_start(1000)
        }

        console.log(`Current timer: ${stats.time_current}`)
    }

    // We want to run that function 60 times a second. Since it's async, we want
    // to wrap it in something that prevents it from being called multiple times
    // at once (race conditions).
    let busy = false
    setInterval(
        () => {
            if (busy) { return; }
            busy = true
            update().finally(() => { busy = false })
        },
        1000 / 60 // update at 60 Hz
    )
</script>
```

Refer to [`client.d.ts`] for documentation on this API.

[`client.d.ts`]: ../supershuckie-frontend-webserver/js/client.d.ts

## REST command reference

If you are not using JS or you do not wish to use the above API, you can also
directly interact with Super Shuckie with these requests.

### enumerate-replays

List all replays for the currently loaded ROM as a JSON string array.

Usage:

- `http://127.0.0.1:30158/enumerate-replays`

### go-to-frame

Go to the desired frame.

Usage:

- `http://127.0.0.1:30158/go-to-frame?frame=FRAME`

Arguments:

| Argument | Default    | Description     |
|----------|------------|-----------------|
| `frame`  | (required) | Frame to go to. |

### increment-counter

Increment a counter. A replay must be recording for this to work.

Usage:

- `http://127.0.0.1:30158/increment-counter?name=NAME`
- `http://127.0.0.1:30158/increment-counter?name=NAME&by=BY`

Arguments:

| Argument | Default    | Description                                                  |
|----------|------------|--------------------------------------------------------------|
| `name`   | (required) | The name of the counter.                                     |
| `by`     | 1          | Amount to increment (or decrement, if negative) the counter. |

### load-replay

Load a replay.

Usage:

- `http://127.0.0.1:30158/load-replay?name=NAME`

Arguments:

| Argument | Default    | Description             |
|----------|------------|-------------------------|
| `name`   | (required) | The name of the replay. |

### mark-start

Mark the start of a replay and enables the timer feature. A replay must be
recording for this to work.

Usage:

- `http://127.0.0.1:30158/mark-start`
- `http://127.0.0.1:30158/mark-start?offset=OFFSET`

Arguments:

| Argument | Default | Description                 |
|----------|---------|-----------------------------|
| `offset` | 0       | The offset in milliseconds. |

### mark-end

Mark the start of a replay and enables the timer feature. A replay must be
recording for this to work.

Usage:

- `http://127.0.0.1:30158/mark-end`

### set-paused

Set whether or not playback is paused.

Usage:

- `http://127.0.0.1:30158/set-paused?paused=PAUSED`

Arguments:

| Argument | Default    | Description                         |
|----------|------------|-------------------------------------|
| `paused` | (required) | `true` to pause, `false` to unpause |


### set-playback-speed

Set playback speed. This speed setting does not persist, so any speed change
will override this.

Usage:

- `http://127.0.0.1:30158/set-playback-speed?speed=SPEED`

Arguments:

| Argument | Default    | Description                             |
|----------|------------|-----------------------------------------|
| `speed`  | (required) | The speed multiplier (i.e. 1.0 = 100%). |

### stats

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
| `is_playback_finished` | `boolean`                | `true` if the current replay has reached the end, `false` if not (or no replay playing).                            |
| `current_speed`        | `number`                 | The current playback speed multiplier.                                                                              |
| `counters`             | `Record<string, number>` | The current values of all counters in the currently playing/recording replay.                                       |
