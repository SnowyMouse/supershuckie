#ifndef __SUPERSHUCKIE_CONTROLLER_SETTING_DIALOG_HPP__
#define __SUPERSHUCKIE_CONTROLLER_SETTING_DIALOG_HPP__

#include <QDialog>
#include <QLineEdit>
#include <vector>
#include <memory>
#include <QTimer>
#include <map>

#include <supershuckie/control_settings.h>

class QComboBox;
class QString;

namespace SuperShuckie64 {

class MainWindow;

class ControlSettingsSetting;

class ControlsSettingsWindow : public QDialog {
    Q_OBJECT;
    friend ControlSettingsSetting;
    friend MainWindow;
public:
    typedef std::map<std::uint8_t, std::pair<const char *, std::unique_ptr<SuperShuckieControlSettingsRaw, decltype(&supershuckie_control_settings_free)>>> SettingsMap;
    ControlsSettingsWindow(MainWindow *parent, SettingsMap settings);
    int exec() override;
private:
    std::vector<ControlSettingsSetting *> edit_boxes;
    MainWindow *parent;
    SettingsMap settings;
    SuperShuckieControlSettingsRaw *current_settings = nullptr;
    const char *ss_device_name();
    std::string ss_device_back;
    QTimer ticker;
    QComboBox *selected_emulator;
    QComboBox *selected_device;
    void tick();
private slots:
    void update_textboxes();
};

class ControlSettingsSetting : public QLineEdit {
    Q_OBJECT;
    friend ControlsSettingsWindow;
private:
    ControlSettingsSetting(ControlsSettingsWindow *window, SuperShuckieControlType control_type, SuperShuckieControlModifier control_modifier);

    ControlsSettingsWindow *window;

    SuperShuckieControlType control_type;
    SuperShuckieControlModifier control_modifier;

    void mousePressEvent(QMouseEvent *event) override;
    void keyPressEvent(QKeyEvent *event) override;

    void update_parent();
};

}

#endif
