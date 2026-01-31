// client.js - see client.d.ts for documentation
//

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
        if(name === "" || typeof name != "string") {
            throw new TypeError("increment_counter counter must be a non-empty string")
        }
        if(typeof by !== "number" || !Number.isInteger(by)) {
            throw new TypeError("increment_counter by must be an integer")
        }

        const result = await fetch(this._construct_url(`increment-counter?name=${encodeURIComponent(name)}&by=${by}`))
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
