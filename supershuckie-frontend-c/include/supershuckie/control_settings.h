#ifndef __SUPERSHUCKIE_CONTROL_SETTINGS_H_
#define __SUPERSHUCKIE_CONTROL_SETTINGS_H_

#ifdef __cplusplus
extern "C" {
#endif

#include <stdlib.h>
#include <stdint.h>
#include <stdbool.h>

/**
 * A map of all control settings.
 *
 * Safety: This can never be null unless passed into supershuckie_control_settings_free.
 */
struct SuperShuckieControlSettingsRaw;

/**
 * Refers to a control type of the emulator (e.g. D-Pad, Start, rapid fire, etc.), NOT on the physical device.
 */
typedef uint32_t SuperShuckieControlType;

/**
 * Refers to a control modifier.
 *
 * Note: This can only be `0` (normal) if the given control type is not a button.
 */
typedef uint32_t SuperShuckieControlModifier;

/**
 * Return the name of the control, or null.
 */
const char *supershuckie_control_settings_control_name(SuperShuckieControlType control);

/**
 * Return the name of the modifier, or null.
 */
const char *supershuckie_control_settings_modifier_name(SuperShuckieControlModifier modifier);

/**
 * Return true if the control type corresponds to a button (thus modifier can be values besides 0).
 */
bool supershuckie_control_settings_control_is_button(SuperShuckieControlType control);

/**
 * Clear controls for a device.
 *
 * If device_name is null, the keyboard will be used.
 *
 * Safety:
 * - device_name, if non-null, must be a null terminated UTF-8 string
 */
void supershuckie_control_settings_clear_controls_for_device(
    struct SuperShuckieControlSettingsRaw *settings,
    const char *device_name,
    uint32_t control,
    uint32_t modifier
);

/**
 * Get controls for a device, returning the total number of controls.
 *
 * If device_name is null, the keyboard will be used.
 *
 * Safety:
 * - device_name, if non-null, must be a null terminated UTF-8 string
 * - input_codes must point to a free buffer of int32_t's of at least input_codes_count length (input_codes CAN be null if this is 0)
 */
size_t supershuckie_control_settings_get_controls_for_device(
    const struct SuperShuckieControlSettingsRaw *settings,
    const char *device_name,
    bool is_axis,
    uint32_t control,
    uint32_t modifier,
    int32_t *input_codes,
    size_t input_codes_count
);

/**
 * Set controls for a device.
 *
 * If device_name is null, the keyboard will be used.
 *
 * Safety:
 * - device_name, if non-null, must be a null terminated UTF-8 string
 */
void supershuckie_control_settings_set_control_for_device(
    struct SuperShuckieControlSettingsRaw *settings,
    const char *device_name,
    bool is_axis,
    int32_t code,
    uint32_t control,
    uint32_t modifier
);

/**
 * Free the settings map.
 *
 * Safety:
 * - A pointer may only be freed once (unless the pointer is null)
 */
void supershuckie_control_settings_free(struct SuperShuckieControlSettingsRaw *array);

#ifdef __cplusplus
}
#endif

#endif
