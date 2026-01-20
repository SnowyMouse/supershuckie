#ifndef __SUPERSHUCKIE_FRONTEND_H_
#define __SUPERSHUCKIE_FRONTEND_H_

#ifdef __cplusplus
extern "C" {
#endif

struct SuperShuckieStringArrayRaw;
struct SuperShuckieControlSettingsRaw;

#include <stdlib.h>
#include <stdint.h>
#include <stdbool.h>

/**
 * Represents an opaque SuperShuckie frontend created with supershuckie_frontend_new() and freed with supershuckie_frontend_free().
 *
 * EXCEPT for supershuckie_frontend_free, no functions that take a pointer to a frontend accept a null SuperShuckieFrontendRaw pointer.
 */
struct SuperShuckieFrontendRaw;

typedef uint32_t SuperShuckieConnectedControllerIndex;

struct SuperShuckieScreenData {
    uint32_t width;
    uint32_t height;
    uint32_t encoding;
};

enum SuperShuckieReplayState {
    SuperShuckieReplayState__NoReplay,
    SuperShuckieReplayState__Recording,
    SuperShuckieReplayState__Playback
};

enum SuperShuckieEmulatorType {
    SuperShuckieEmulatorType__GameBoy,
    SuperShuckieEmulatorType__GameBoySGB2,
    SuperShuckieEmulatorType__GameBoyColor,
    SuperShuckieEmulatorType__GameBoyAdvance,
    SuperShuckieEmulatorType__NintendoDS
};

typedef void (*SuperShuckieRefreshScreensCallback)(void *user_data, size_t screen_count, const uint32_t *const *pixels);
typedef void (*SuperShuckieChangeVideoModeCallback)(void *user_data, size_t screen_count, const struct SuperShuckieScreenData *screen_data, uint8_t scaling);

struct SuperShuckieFrontendCallbacks {
    void *user_data;

    SuperShuckieRefreshScreensCallback refresh_screens;
    SuperShuckieChangeVideoModeCallback change_video_mode;
};

/**
 * Initialize a new frontend.
 *
 * Safety:
 * - All pointers must point to valid data.
 */
struct SuperShuckieFrontendRaw *supershuckie_frontend_new(
    const char *data_dir_path,
    const char *config_dir_path,
    const struct SuperShuckieFrontendCallbacks *callbacks
);

/**
 * Set the current state for a keyboard key press, if any.
 */
void supershuckie_frontend_key_press(
    struct SuperShuckieFrontendRaw *frontend,
    int32_t key_code,
    bool pressed
);

/**
 * Set the current button value for the given controller.
 */
void supershuckie_frontend_button_press(
    struct SuperShuckieFrontendRaw *frontend,
    SuperShuckieConnectedControllerIndex controller,
    int32_t button,
    bool pressed
);

/**
 * Set the current axis value for the given controller.
 */
void supershuckie_frontend_axis(
    struct SuperShuckieFrontendRaw *frontend,
    SuperShuckieConnectedControllerIndex controller,
    int32_t axis,
    double value
);

/**
 * Set whether or not SGB is enabled.
 */
void supershuckie_frontend_set_sgb_enabled(struct SuperShuckieFrontendRaw *frontend, bool enabled);

/**
 * Get whether or not SGB is enabled.
 */
bool supershuckie_frontend_is_sgb_enabled(struct SuperShuckieFrontendRaw *frontend);

enum SuperShuckieGBCMode {
    SuperShuckieGBCMode__AlwaysGBC = 0,
    SuperShuckieGBCMode__GBInGBMode = 1,
    SuperShuckieGBCMode__AlwaysGB = 2
};

/**
 * Set the GBC mode.
 */
void supershuckie_frontend_set_gbc_mode(struct SuperShuckieFrontendRaw *frontend, uint32_t mode);

/**
 * Get the GBC mode.
 */
uint32_t supershuckie_frontend_get_gbc_mode(struct SuperShuckieFrontendRaw *frontend);

/**
 * Get whether or not SGB is enabled.
 */
bool supershuckie_frontend_is_sgb_enabled(struct SuperShuckieFrontendRaw *frontend);

/**
 * Set whether or not the frontend is paused.
 */
void supershuckie_frontend_set_paused(struct SuperShuckieFrontendRaw *frontend, bool paused);

/**
 * Manually invoke the refresh screens callback even if no updates have occurred.
 */
void supershuckie_frontend_force_refresh_screens(struct SuperShuckieFrontendRaw *frontend);

/**
 * Set the video scale.
 *
 * If scale is 0, it will default to 1.
 */
void supershuckie_frontend_set_video_scale(struct SuperShuckieFrontendRaw *frontend, uint8_t scale);

/**
 * Get the current speed settings.
 *
 * Safety:
 * - base and/or turbo can be null
 */
void supershuckie_frontend_get_speed_settings(const struct SuperShuckieFrontendRaw *frontend, double *base, double *turbo);

/**
 * Set the current speed settings.
 */
void supershuckie_frontend_set_speed_settings(struct SuperShuckieFrontendRaw *frontend, double base, double turbo);

/**
 * Get the setting, or null if no setting is set.
 *
 * Safety:
 * - setting must not be null
 * - The returned value may no longer be valid once any future call to this API is made.
 */
const char *supershuckie_frontend_get_custom_setting(const struct SuperShuckieFrontendRaw *frontend, const char *setting);

/**
 * Set the setting to the value, or null to unset.
 *
 * Safety:
 * - setting must not be null
 */
void supershuckie_frontend_set_custom_setting(const struct SuperShuckieFrontendRaw *frontend, const char *setting, const char *value);

/**
 * Start recording a replay with the given name, or null to use a default name.
 *
 * If true is returned, the name of the replay (besides the extension) will be written to result (ensure it is long enough).
 *
 * If false is returned, an error will be written.
 *
 * Safety:
 * - result must not be null and must be at least result_len bytes long.
 */
bool supershuckie_frontend_start_recording_replay(struct SuperShuckieFrontendRaw *frontend, const char *name, char *result, size_t result_len);

/**
 * Stop recording a replay.
 */
void supershuckie_frontend_stop_recording_replay(struct SuperShuckieFrontendRaw *frontend);

/**
 * Get whether or not Poke-A-Byte is enabled.
 *
 * If false, error may be filled with error data if there is any error data (or it will be empty if it is simply not
 * enabled).
 *
 * Safety:
 * - error must not be null and must be at least error_len bytes long.
 */
bool supershuckie_frontend_is_pokeabyte_enabled(const struct SuperShuckieFrontendRaw *frontend, char *error, size_t error_len);

/**
 * Set whether or not Poke-A-Byte is enabled.
 *
 * Returns false if an error occurs, filling the error buffer with the error.
 *
 * Safety:
 * - error must not be null and must be at least error_len bytes long.
 */
bool supershuckie_frontend_set_pokeabyte_enabled(struct SuperShuckieFrontendRaw *frontend, bool enabled, char *error, size_t error_len);

/**
 * Get whether or not remote commands are enabled.
 *
 * If false, error may be filled with error data if there is any error data (or it will be empty if it is simply not
 * enabled).
 *
 * Safety:
 * - error must not be null and must be at least error_len bytes long.
 */
bool supershuckie_frontend_get_external_commands_enabled(const struct SuperShuckieFrontendRaw *frontend, char *error, size_t error_len);

/**
 * Set whether or not remote commands are enabled.
 *
 * Returns false if an error occurs, filling the error buffer with the error.
 *
 * Safety:
 * - error must not be null and must be at least error_len bytes long.
 */
bool supershuckie_frontend_set_external_commands_enabled(const struct SuperShuckieFrontendRaw *frontend, bool enabled, char *error, size_t error_len);

/**
 * Return true if the emulator is currently manually paused.
 */
bool supershuckie_frontend_is_paused(const struct SuperShuckieFrontendRaw *frontend);

/**
 * Get the currently recorded replay file, or nullptr if none.
 */
const char *supershuckie_frontend_get_recording_replay_file(const struct SuperShuckieFrontendRaw *frontend);

/**
 * Create a save state of the given name, or null to use a default name.
 *
 * If true is returned, the name of the save state (besides the extension) will be written to result (ensure it is long enough).
 *
 * If false is returned, an error will be written.
 *
 * Safety:
 * - result must not be null and must be at least result_len bytes long.
 */
bool supershuckie_frontend_create_save_state(struct SuperShuckieFrontendRaw *frontend, const char *name, char *result, size_t result_len);

/**
 * Load a save state of the given name.
 *
 * If false is returned, an error will be written UNLESS it was because the save state did not exist, in which case the
 * error will be empty.
 *
 * Safety:
 * - name must not be null
 * - error must be at least result_len bytes long.
 */
bool supershuckie_frontend_load_save_state(struct SuperShuckieFrontendRaw *frontend, const char *name, char *error, size_t error_len);

/**
 * Undo loading a save state, storing a backup of the current state in the stack.
 *
 * Returns true if successful or false if the end of the stack has been reached.
 */
bool supershuckie_frontend_undo_load_save_state(struct SuperShuckieFrontendRaw *frontend);

/**
 * Redo loading a save state, storing a backup of the current state in the stack.
 *
 * Returns true if successful or false if the end of the stack has been reached.
 */
bool supershuckie_frontend_redo_load_save_state(struct SuperShuckieFrontendRaw *frontend);

/**
 * Load the given ROM, returning true or false depending on whether or not it was successfully loaded.
 *
 * Safety:
 * - path must be null-terminated, UTF-8
 * - error must point to a buffer of at least `error_len` bytes (it can be null if error_len is 0)
 */
bool supershuckie_frontend_load_rom(struct SuperShuckieFrontendRaw *frontend, const char *path, char *error, size_t error_len);

/**
 * Write SRAM to disk, returning true if successful.
 *
 * Safety:
 * - error must be at least result_len bytes long.
 */
bool supershuckie_frontend_save_sram(struct SuperShuckieFrontendRaw *frontend, char *error, size_t error_len);

/**
 * Set the auto stop playback setting.
 */
void supershuckie_frontend_set_auto_stop_playback_on_input_setting(struct SuperShuckieFrontendRaw *frontend, bool new_setting);

/**
 * Get the auto stop playback setting.
 */
bool supershuckie_frontend_get_auto_stop_playback_on_input_setting(const struct SuperShuckieFrontendRaw *frontend);

/**
 * Set the auto unpause setting.
 */
void supershuckie_frontend_set_auto_unpause_on_input_setting(struct SuperShuckieFrontendRaw *frontend, bool new_setting);

/**
 * Get the auto unpause setting.
 */
bool supershuckie_frontend_get_auto_unpause_on_input_setting(const struct SuperShuckieFrontendRaw *frontend);

/**
 * Set the auto pause on record setting.
 */
void supershuckie_frontend_set_auto_pause_on_record_setting(struct SuperShuckieFrontendRaw *frontend, bool new_setting);

/**
 * Get the auto pause on record setting.
 */
bool supershuckie_frontend_get_auto_pause_on_record_setting(const struct SuperShuckieFrontendRaw *frontend);

/**
 * Set the current frame for playback.
 */
void supershuckie_frontend_set_playback_frame(struct SuperShuckieFrontendRaw *frontend, uint32_t frame);

/**
 * Advance or go back a set number of frames.
 */
void supershuckie_frontend_advance_playback_frames(struct SuperShuckieFrontendRaw *frontend, int32_t delta);

/**
 * Set paused (temporarily)
 */
void supershuckie_frontend_set_playback_frozen(struct SuperShuckieFrontendRaw *frontend, bool paused);

/**
 * Get the replay playback stats, returning true if currently playing back a replay.
 *
 * total_frames and total_milliseconds, if non-null, will be written their respective values.
 */
bool supershuckie_frontend_get_replay_playback_time(
    const struct SuperShuckieFrontendRaw *frontend,
    uint32_t *total_frames,
    uint32_t *total_milliseconds
);

/**
 * Get the number of milliseconds and frames elapsed.
 *
 * elapsed_frames and elapsed_milliseconds, if non-null, will be written their respective values.
 */
void supershuckie_frontend_get_elapsed_time(
    const struct SuperShuckieFrontendRaw *frontend,
    uint32_t *elapsed_frames,
    uint32_t *elapsed_milliseconds
);

/**
 * Load the given replay, returning true or false depending on whether or not it was successfully loaded.
 *
 * Safety:
 * - path must be null-terminated, UTF-8
 * - error must point to a buffer of at least `error_len` bytes (it can be null if error_len is 0)
 */
bool supershuckie_frontend_load_replay(
    struct SuperShuckieFrontendRaw *frontend,
    const char *name,
    bool ignore_some_errors,
    char *error,
    size_t error_len
);

/**
 * Stop the currently playing replay, if any.
 */
void supershuckie_frontend_stop_replay_playback(struct SuperShuckieFrontendRaw *frontend);

/**
 * If there is a ROM running, return the name. Otherwise, return null.
 */
const char *supershuckie_frontend_get_rom_name(const struct SuperShuckieFrontendRaw *frontend);

/**
 * Write settings to the given settings file.
 */
void supershuckie_frontend_write_settings(const struct SuperShuckieFrontendRaw *frontend);

/**
 * Return true if there is currently a game running.
 */
bool supershuckie_frontend_is_game_running(const struct SuperShuckieFrontendRaw *frontend);

/**
 * Unload the current ROM, if any.
 *
 * Will also try to save the SRAM.
 */
void supershuckie_frontend_close_rom(struct SuperShuckieFrontendRaw *frontend);

/**
 * Unload the current ROM, if any.
 *
 * Does NOT save the SRAM.
 */
void supershuckie_frontend_unload_rom(struct SuperShuckieFrontendRaw *frontend);

/**
 * Load a save save file, automatically saving the current SRAM before switching.
 *
 * If initialize is true, the save file will be deleted if it exists.
 *
 * Safety:
 * - save_name must be null-terminated UTF-8
 */
void supershuckie_frontend_load_or_create_save_file(struct SuperShuckieFrontendRaw *frontend, const char *save_name, bool initialize);

/**
 * Set the current save file without reloading anything.
 *
 * Safety:
 * - save_name must be null-terminated UTF-8
 */
void supershuckie_frontend_set_current_save_file(struct SuperShuckieFrontendRaw *frontend, const char *save_name);

/**
 * Hard reset the console, simulating switching off/on.
 */
void supershuckie_frontend_hard_reset_console(struct SuperShuckieFrontendRaw *frontend);

/**
 * Should be called regularly.
 *
 * Returns false if an error occurred, with the error written to `error`.
 *
 * Safety:
 * - error must point to a buffer of at least `error_len` bytes
 */
bool supershuckie_frontend_tick(struct SuperShuckieFrontendRaw *frontend, char *error, size_t error_len);

/**
 * Get all replays for the given rom, or the currently loaded ROM if no ROM passed in.
 *
 * This array must be freed with supershuckie_stringarray_free
 */
struct SuperShuckieStringArrayRaw *supershuckie_frontend_get_all_replays_for_rom(const struct SuperShuckieFrontendRaw *frontend, const char *rom);

/**
 * Get all save states for the given rom, or the currently loaded ROM if no ROM passed in.
 *
 * This array must be freed with supershuckie_stringarray_free
 */
struct SuperShuckieStringArrayRaw *supershuckie_frontend_get_all_save_states_for_rom(const struct SuperShuckieFrontendRaw *frontend, const char *rom);

/**
 * Get all saves for the given rom, or the currently loaded ROM if no ROM passed in.
 *
 * This array must be freed with supershuckie_stringarray_free
 */
struct SuperShuckieStringArrayRaw *supershuckie_frontend_get_all_saves_for_rom(const struct SuperShuckieFrontendRaw *frontend, const char *rom);

/**
 * Copy the control settings.
 *
 * This pointer must be freed with supershuckie_control_settings_free to avoid memory leaks.
 */
SuperShuckieControlSettingsRaw *supershuckie_frontend_get_control_settings(const struct SuperShuckieFrontendRaw *frontend, uint8_t emulator_type);

/**
 * Overwrite the control settings.
 */
void supershuckie_frontend_set_control_settings(struct SuperShuckieFrontendRaw *frontend, const SuperShuckieControlSettingsRaw *settings, uint8_t emulator_type);

/**
 * Get the name of the emulator type.
 *
 * Returns null if the emulator type is not valid.
 */
const char *supershuckie_frontend_get_emulator_type_name(uint8_t emulator_type);

/**
 * Return true if the emulator type shares another system's config.
 */
bool supershuckie_frontend_emulator_type_uses_shared_config(uint8_t emulator_type);

/**
 * Get a list of all controllers.
 *
 * This array must be freed with supershuckie_stringarray_free
 */
SuperShuckieStringArrayRaw *supershuckie_frontend_get_connected_controllers(
    struct SuperShuckieFrontendRaw *frontend
);

/**
 * Connect a controller.
 *
 * Safety: The name must be a null-terminated UTF-8 string
 */
SuperShuckieConnectedControllerIndex supershuckie_frontend_connect_controller(
    struct SuperShuckieFrontendRaw *frontend,
    const char *name
);

/**
 * Disconnect the controller at the given index.
 */
void supershuckie_frontend_disconnect_controller(
    struct SuperShuckieFrontendRaw *frontend,
    SuperShuckieConnectedControllerIndex controller
);

/**
 * Get the name of the controller, returning null if the index is invalid.
 */
const char *supershuckie_frontend_get_name_of_controller(
    const struct SuperShuckieFrontendRaw *frontend,
    SuperShuckieConnectedControllerIndex controller
);

/**
 * Get the replay state
 */
enum SuperShuckieReplayState supershuckie_frontend_get_replay_state(const struct SuperShuckieFrontendRaw *frontend);

/**
 * Set the touch
 */
void supershuckie_frontend_set_touch(struct SuperShuckieFrontendRaw *frontend, bool enabled, uint8_t x, uint8_t y);

struct SuperShuckieNintendoDSDate {
    uint16_t year;
    uint8_t month;
    uint8_t day;
    uint8_t hour;
    uint8_t minute;
    uint8_t second;
};

/**
 * Get the Nintendo DS date
 */
void supershuckie_frontend_get_nds_date(const struct SuperShuckieFrontendRaw *frontend, struct SuperShuckieNintendoDSDate *date);

/**
 * Set the Nintendo DS date
 */
void supershuckie_frontend_set_nds_date(struct SuperShuckieFrontendRaw *frontend, const struct SuperShuckieNintendoDSDate *date);

/**
 * Get if JIT is enabled for the DS.
 */
bool supershuckie_frontend_get_nds_jit(const struct SuperShuckieFrontendRaw *frontend);

/**
 * Set if JIT is enabled for the DS.
 */
void supershuckie_frontend_set_nds_jit(struct SuperShuckieFrontendRaw *frontend, bool enabled);

/**
 * Get if speed changes from the replay should be ignored when playing back replays.
 */
bool supershuckie_frontend_get_ignore_speed_changes_in_replay(const struct SuperShuckieFrontendRaw *frontend);

/**
 * Set if speed changes from the replay should be ignored when playing back replays.
 */
void supershuckie_frontend_set_ignore_speed_changes_in_replay(struct SuperShuckieFrontendRaw *frontend, bool ignored);

/**
 * Get all recent ROMs.
 *
 * The resulting string array must be freed with supershuckie_stringarray_free.
 */
struct SuperShuckieStringArrayRaw *supershuckie_frontend_get_recent_roms(const struct SuperShuckieFrontendRaw *frontend);

/**
 * Clear all recent ROMs.
 */
struct SuperShuckieStringArrayRaw *supershuckie_frontend_clear_recent_roms(struct SuperShuckieFrontendRaw *frontend);

/**
 * Get the current data directory, returning the length.
 *
 * Safety:
 * - `dir` must be valid for at least `dir_len` bytes, only being null if dir_len is 0.
 */
size_t supershuckie_frontend_get_current_data_directory(const struct SuperShuckieFrontendRaw *frontend, char *dir, size_t dir_len);

/**
 * Reload the current core.
 *
 * This will end any replay and automatically save the current game.
 */
void supershuckie_frontend_reload_core(struct SuperShuckieFrontendRaw *frontend);

/**
 * Get the current emulator type.
 *
 * Returns -1 if not running a game.
 */
enum SuperShuckieEmulatorType supershuckie_frontend_get_emulator_type(const struct SuperShuckieFrontendRaw *frontend);

/**
 * Free the core
 *
 * Safety:
 * - frontend must either be created with supershuckie_frontend_new OR it can be null
 * - frontend, if non-null, may only be freed once
 */
void supershuckie_frontend_free(struct SuperShuckieFrontendRaw *frontend);

#ifdef __cplusplus
}
#endif

#endif
