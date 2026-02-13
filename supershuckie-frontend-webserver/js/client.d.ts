/*!
 * client.d.ts
 *
 * Copyright 2026 SnowyMouse
 *
 * The Super Shuckie JavaScript API (client.js and client.d.ts) are licensed
 * under version 3 of the GNU GPL, just like the rest of Super Shuckie.
 *
 * HOWEVER, you may alternatively use, copy, link with, and/or distribute
 * the Super Shuckie JavaScript API under the terms of Version 2.0 of the
 * Apache License, obtainable at http://www.apache.org/licenses/LICENSE-2.0
 */

/**
 * Client interface
 */
export class SuperShuckieClient {
    /**
     * Create a client for the given server
     * @param server to use (default = "http://127.0.0.1:30158")
     */
    constructor(server?: string)

    /**
     * Mark the start of a replay
     * @param offset offset in milliseconds (default = 0)
     */
    mark_start(offset?: number): Promise<void>

    /**
     * Mark the end of a replay
     */
    mark_end(): Promise<void>

    /**
     * Increment/decrement the given counter
     * @param name name of counter
     * @param by amount to increment or decrement (default = 1)
     */
    increment_counter(name: string, by?: number): Promise<void>

    /**
     * Get the stats
     */
    stats(): Promise<SuperShuckieStats>

    /**
     * List all replays
     */
    enumerate_replays(): Promise<string[]>

    /**
     * Load the replay
     * @param name name of replay
     */
    load_replay(name: string): Promise<void>

    /**
     * Set the speed
     * @param speed speed multiplayer (1 = 100%, 2 = 200%, 0.5 = 50%, etc.)
     */
    set_playback_speed(speed: number): Promise<void>

    /**
     * Go to the given frame index in a replay
     * @param frame frame index
     */
    go_to_frame(frame: number): Promise<void>

    /**
     * Set whether or not playback is paused.
     * @param paused if true, pause. otherwise, unpause
     */
    set_paused(paused: boolean): Promise<void>
}

/**
 * Stats (see external_commands.md)
 */
export interface SuperShuckieStats {
    time_start: number | null,
    time_end: number | null,
    time_offset: number | null,
    time_current: number | null,

    total_elapsed_time: number,
    total_elapsed_frames: number,

    is_recording: boolean,
    is_playing_back: boolean,
    is_paused: boolean,
    is_playback_finished: boolean,

    counters: Record<string, number>,

    current_speed: number
}

/**
 * Error that may occur from a rest request
 */
export class SuperShuckieError extends Error {

}
