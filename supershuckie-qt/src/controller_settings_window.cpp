#include "controller_settings_window.hpp"
#include "main_window.hpp"

#include <SDL3/SDL.h>
#include <QGridLayout>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QComboBox>
#include <QString>

using namespace SuperShuckie64;

ControlsSettingsWindow::ControlsSettingsWindow(MainWindow *parent, SuperShuckieControlSettingsRaw *settings): QDialog(parent), parent(parent), settings(settings, supershuckie_control_settings_free) {
    this->setWindowTitle("Controls settings");
    
    SuperShuckieControlType control_types = 0;
    SuperShuckieControlModifier control_modifiers = 0;
    const char *label = nullptr;

    auto *layout = new QGridLayout(this);
    this->selected_device = new QComboBox(this);
    this->selected_device->addItem("Keyboard");
    
    auto devices = wrap_array_std(supershuckie_frontend_get_connected_controllers(this->parent->frontend));
    for(auto &d: devices) {
        this->selected_device->addItem(d.c_str());
    }

    // Show the user's first controller if they have one
    if(this->selected_device->count() > 1) {
        this->selected_device->setCurrentIndex(1);
    }

    connect(this->selected_device, SIGNAL(currentIndexChanged(int)), this, SLOT(update_textboxes()));

    int control_box_y_offset = 100;
    
    for(control_types = 0; (label = supershuckie_control_settings_control_name(control_types)) != nullptr; control_types++) {
        auto *name = new QLabel(label, this);
        layout->addWidget(name, control_box_y_offset + control_types + 1, 0);
    }
    
    for(control_modifiers = 0; (label = supershuckie_control_settings_modifier_name(control_modifiers)) != nullptr; control_modifiers++) {
        auto *name = new QLabel(label, this);
        
        int y = control_box_y_offset;
        int x = control_modifiers + 1;
        layout->addWidget(name, y, x);

        for(SuperShuckieControlType control_type = 0; control_type < control_types; control_type++) {
            if(!supershuckie_control_settings_control_is_button(control_type) && control_modifiers != 0) {
                continue;
            }

            auto *edit = new ControlSettingsSetting(this, control_type, control_modifiers);
            this->edit_boxes.emplace_back(edit);
            layout->addWidget(edit, control_box_y_offset + control_type + 1, x);
        }
    }

    int offset_for_remaining_things = control_box_y_offset + control_types + 1;
    int width_span = control_modifiers + 1;

    auto *note = new QLabel("Left-click to select a setting. Right-click to clear.", this);
    note->setAlignment(Qt::AlignHCenter);
    note->setAttribute(Qt::WA_MacSmallSize);
    layout->addWidget(note, offset_for_remaining_things++, 0, 1, width_span);

    auto *save = new QPushButton("OK", this);
    layout->addWidget(save, offset_for_remaining_things++, 0, 1, width_span);
    connect(save, SIGNAL(clicked()), this, SLOT(accept()));

    layout->addWidget(this->selected_device, 0, 0, 1, width_span);

    this->setFixedSize(this->sizeHint());
    this->update_textboxes();
    this->ticker.setInterval(1);
    this->ticker.callOnTimeout(this, &ControlsSettingsWindow::tick);
    this->ticker.start();
}

void ControlsSettingsWindow::tick() {
    if(!this->isVisible()) {
        return;
    }

    auto *current_device = this->ss_device_name();

    while(true) {
        auto sdl_event = this->parent->sdl.next();
        switch(sdl_event.discriminator) {
            case SDLEventWrapperAction::SDLEventWrapper_NoOp:
                return;
            case SDLEventWrapperAction::SDLEventWrapper_Quit:
                this->reject();
                return;
            case SDLEventWrapperAction::SDLEventWrapper_Axis: {
                auto &axis_event = sdl_event.axis;
                auto *name = axis_event.controller->name.c_str();

                if(current_device == nullptr || std::strcmp(current_device, name) != 0) {
                    break;
                }

                auto axis = axis_event.axis;
                auto value = axis_event.value;

                if(value < 0.5 && value > -0.5) {
                    break;
                }

                for(auto &box : edit_boxes) {
                    if(box->hasFocus()) {
                        supershuckie_control_settings_set_control_for_device(this->settings.get(), name, true, axis, box->control_type, box->control_modifier);
                        this->update_textboxes();
                        break;
                    }
                }

                break;
            }
            case SDLEventWrapperAction::SDLEventWrapper_Button: {
                auto &button_event = sdl_event.button;
                auto *name = button_event.controller->name.c_str();

                if(current_device == nullptr || std::strcmp(current_device, name) != 0) {
                    break;
                }

                auto button = button_event.button;
                auto pressed = button_event.pressed;

                if(!pressed) {
                    break;
                }

                for(auto &box : edit_boxes) {
                    if(box->hasFocus()) {
                        supershuckie_control_settings_set_control_for_device(this->settings.get(), name, false, button, box->control_type, box->control_modifier);
                        this->update_textboxes();
                        break;
                    }
                }

                break;
            }
        }
    }
}

int ControlsSettingsWindow::exec() {
    this->parent->stop_timer();
    int return_value = QDialog::exec();
    this->parent->start_timer();
    return return_value;
}

ControlSettingsSetting::ControlSettingsSetting(ControlsSettingsWindow *window, SuperShuckieControlType control_type, SuperShuckieControlModifier control_modifier):
    QLineEdit(window),
    window(window),
    control_type(control_type),
    control_modifier(control_modifier) {
    this->setContextMenuPolicy(Qt::NoContextMenu);
}

// QString ControlsSettingsWindow::ss_device_name() {
//     if(this->selected_device->currentIndex() == 0) {
//         return "";
//     }
//     return this->selected_device->currentText();
// }

void ControlSettingsSetting::mousePressEvent(QMouseEvent *event) {
    auto button = event->button();
    auto *device = this->window->ss_device_name();

    if(button & Qt::RightButton) {
        event->ignore();

        supershuckie_control_settings_clear_controls_for_device(
            this->window->settings.get(),
            device,
            this->control_type,
            this->control_modifier
        );
        this->window->update_textboxes();
    }
    else if(button & Qt::LeftButton) {

    }
}

void ControlSettingsSetting::keyPressEvent(QKeyEvent *event) {
    event->ignore();

    auto device = this->window->ss_device_name();
    if(device != nullptr) {
        return;
    }

    supershuckie_control_settings_set_control_for_device(
        this->window->settings.get(),
        device,
        false,
        event->key(),
        this->control_type,
        this->control_modifier
    );

    this->window->update_textboxes();
}

const char *ControlsSettingsWindow::ss_device_name() {
    return this->selected_device->currentIndex() == 0 ? nullptr : this->ss_device_back.c_str();
}

void ControlsSettingsWindow::update_textboxes() {
    this->ss_device_back = this->selected_device->currentText().toStdString();

    std::vector<std::int32_t> buffer_button;
    std::vector<std::int32_t> buffer_axis;

    auto *device = this->ss_device_name();

    for(ControlSettingsSetting *setting: this->edit_boxes) {
        auto button_len = supershuckie_control_settings_get_controls_for_device(
            this->settings.get(),
            device,
            false,
            setting->control_type,
            setting->control_modifier,
            nullptr,
            0
        );
        buffer_button.resize(button_len);
        supershuckie_control_settings_get_controls_for_device(
            this->settings.get(),
            device,
            false,
            setting->control_type,
            setting->control_modifier,
            buffer_button.data(),
            buffer_button.size()
        );

        if(device != nullptr) {
            auto axis_len = supershuckie_control_settings_get_controls_for_device(
                this->settings.get(),
                device,
                true,
                setting->control_type,
                setting->control_modifier,
                nullptr,
                0
            );
            buffer_axis.resize(axis_len);
            supershuckie_control_settings_get_controls_for_device(
                this->settings.get(),
                device,
                false,
                setting->control_type,
                setting->control_modifier,
                buffer_axis.data(),
                buffer_axis.size()
            );
        }
        
        QString label;

        if(device == nullptr) {
            for(auto button : buffer_button) {
                auto name = QKeySequence(button).toString();
                if(label.isEmpty()) {
                    label = name;
                }
                else {
                    label += ", ";
                    label += name;
                }
            }
        }
        else {
            char name_fmt[256];
            for(auto button : buffer_button) {
                auto *name = SDL_GetGamepadStringForButton(static_cast<SDL_GamepadButton>(button));
                if(name == nullptr) {
                    std::snprintf(name_fmt, sizeof(name_fmt), "Button #%d", button);
                    name = name_fmt;
                }

                if(label.isEmpty()) {
                    label = name;
                }
                else {
                    label += ", ";
                    label += name;
                }
            }
            for(auto axis : buffer_axis) {
                auto *name = SDL_GetGamepadStringForAxis(static_cast<SDL_GamepadAxis>(axis));
                if(name == nullptr) {
                    std::snprintf(name_fmt, sizeof(name_fmt), "Axis #%d", axis);
                    name = name_fmt;
                }

                if(label.isEmpty()) {
                    label = name;
                }
                else {
                    label += ", ";
                    label += name;
                }
            }
        }

        setting->setText(label);
    }
}
