# Super Shuckie external commands

You can control Super Shuckie using REST requests. To enable this feature, you
will need the feature enabled inside the application itself.

The server is `127.0.0.1:30158`

## JS API

To make things simple, you can use the JavaScript API. Here is some example
usage.

### Usage

First, you will want to import it. There are a few ways you can do this.

#### HTML (loaded from the emulator)

```html
<script type="module" src="http://127.0.0.1:30158/client.js"></script>
```

#### JavaScript (loaded from the emulator)

```javascript
import { SuperShuckieClient } from "http://127.0.0.1:30158/client.js"
```

#### TypeScript (using source code directly)

If you are using TypeScript, you can directly use the definitions and code from
Super Shuckie's source tree at `/supershuckie-frontend-webserver/js`

Ensure that client.d.ts and client.js are in the same directory. Then import it
like this:

```typescript
import { SuperShuckieClient } from "./somepath/client"
```

### Example code

```html
<script type="module" src="http://127.0.0.1:30158/client.js"></script>
<script type="module">
    const shuckie = new SuperShuckieClient();
    
    async function update() {
        const stats = await shuckie.stats()
        
        // start the timer (at a 1 second offset) if it has not already been set
        if (stats.is_recording && stats.time_current == null) {
            await shuckie.mark_start(1000)
        }
        
        console.log(`Current timer: ${stats.time_current} milliseconds`)
    }
    
    // update at 60 Hz
    let busy = false
    setInterval(async function() {
        if(busy) {
            return
        }
        busy = true
        try {
            await update()
        } finally {
            busy = false
        }
    }, 16.67)
</script>

```

Refer to [`client.d.ts`] for documentation on this API.

[`client.d.ts`]: ../supershuckie-frontend-webserver/js/client.d.ts

## REST command reference

If you are not using JS or you do not wish to use the above API, you can also
directly interact with Super Shuckie with these requests.

### Table of contents

- [increment-counter](#increment-counter)
- [mark-start](#mark-start)
- [mark-end](#mark-end)
- [stats](#stats)

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
| `current_speed`        | `number`                 | The current playback speed multiplier.                                                                              |
| `counters`             | `Record<string, number>` | The current values of all counters in the currently playing/recording replay.                                       |
