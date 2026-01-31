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

    counters: Record<string, number>,

    current_speed: number
}

/**
 * Error that may occur from a rest request
 */
export class SuperShuckieError extends Error {

}
