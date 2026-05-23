/*!
 * client.js (see client.d.ts for documentation)
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

"use strict";

export class SuperShuckieClient {
    _server;

    constructor(server = "http://127.0.0.1:30158") {
        this._server = server
    }

    async mark_start(offset = 0) {
        if(typeof offset !== "number" || !Number.isInteger(offset) || offset < 0) {
            throw new TypeError("mark_start offset must be an unsigned integer")
        }

        const result = await fetch(this._construct_url(`mark-start?offset=${offset}`))
        await this._handle_error(result)
    }

    async mark_end() {
        const result = await fetch(this._construct_url(`mark-end`))
        await this._handle_error(result)
    }

    async stats() {
        const result = await fetch(this._construct_url(`stats`))
        await this._handle_error(result)

        return result.json()
    }

    async increment_counter(name, by = 1) {
        if(name === "" || typeof name !== "string") {
            throw new TypeError("increment_counter name must be a non-empty string")
        }
        if(typeof by !== "number" || !Number.isInteger(by)) {
            throw new TypeError("increment_counter by must be an integer")
        }

        const result = await fetch(this._construct_url(`increment-counter?name=${encodeURIComponent(name)}&by=${by}`))
        await this._handle_error(result)
    }

    async enumerate_replays() {
        const result = await fetch(this._construct_url(`enumerate-replays`))
        await this._handle_error(result)

        return result.json()
    }

    async load_replay(name) {
        if(name === "" || typeof name !== "string") {
            throw new TypeError("load_replay name must be a non-empty string")
        }

        const result = await fetch(this._construct_url(`load-replay?name=${encodeURIComponent(name)}`))
        await this._handle_error(result)
    }

    async set_playback_speed(speed) {
        if(typeof speed !== "number" || !Number.isFinite(speed) || speed < 0) {
            throw new TypeError("set_playback_speed speed must be a positive float")
        }

        const result = await fetch(this._construct_url(`set-playback-speed?speed=${speed}`))
        await this._handle_error(result)
    }

    async go_to_frame(frame) {
        if(typeof frame !== "number" || !Number.isInteger(frame) || frame < 0) {
            throw new TypeError("go_to_frame frame must be an unsigned integer")
        }

        const result = await fetch(this._construct_url(`go-to-frame?frame=${frame}`))
        await this._handle_error(result)
    }

    async set_paused(paused) {
        if(typeof paused !== "boolean") {
            throw new TypeError("set_paused paused must be a boolean")
        }

        const result = await fetch(this._construct_url(`set-paused?paused=${paused}`))
        await this._handle_error(result)
    }

    async load_rom(path) {
        if(typeof path !== "string") {
            throw new TypeError("load_rom path must be a string")
        }

        const result = await fetch(this._construct_url(`load-rom?path=${path}`))
        await this._handle_error(result)
    }

    _construct_url(resource) {
        return new URL(resource, this._server)
    }

    async _handle_error(response) {
        if(response.ok) {
            return;
        }

        const j = await response.json()
        throw new SuperShuckieClientRestError(j.error)
    }
}

export class SuperShuckieClientRestError extends Error {
    constructor(message) {
        super(`Super Shuckie error: ${message}`);
    }
}
