#ifndef __SUPERSHUCKIE_MAIN_WINDOW_HPP__
#define __SUPERSHUCKIE_MAIN_WINDOW_HPP__

#include <QMainWindow>
#include <QTimer>
#include <filesystem>
#include <memory>
#include <chrono>
#include <supershuckie/supershuckie.h>
#include "sdl_event_wrapper.hpp"

class QMenu;
class QAction;
class QCloseEvent;
class QLabel;

namespace SuperShuckie64 {

class GameRenderWidget;
class NumberedAction;
class GameSpeedDialog;
class SuperShuckieTimestamp;
class AskForTextDialog;
class SelectItemDialog;
class ControlsSettingsWindow;
class ReplayPlaybackControls;
class NDSDateDialog;
class StringAction;

std::vector<std::string> wrap_array_std(SuperShuckieStringArrayRaw *array);

enum ReplayStatus {
    NoReplay,
    Recording,
    PlayingBack
};

class MainWindow: public QMainWindow {
    Q_OBJECT
    friend GameRenderWidget;
    friend NumberedAction;
    friend GameSpeedDialog;
    friend AskForTextDialog;
    friend SelectItemDialog;
    friend ControlsSettingsWindow;
    friend ReplayPlaybackControls;
    friend NDSDateDialog;
    friend StringAction;
    
public:
    MainWindow();
    ~MainWindow();

    void load_rom(const std::filesystem::path &path);
    void load_rom(const char *path);

private:
    typedef std::chrono::steady_clock clock;

    void set_title(const char *title = "");
    GameRenderWidget *render_widget;
    SuperShuckieFrontendRaw *frontend = nullptr;

    QTimer ticker;

    void tick();

    void set_up_menu();
    QMenuBar *menu_bar;

    QMenu *file_menu;
    QMenu *gameplay_menu;
    QMenu *save_states_menu;
    QMenu *replays_menu;
    QMenu *settings_menu;
    QMenu *recent_roms_menu;

    QMenu *quick_slots;
    QAction *undo_load_save_state;
    QAction *redo_load_save_state;

    QStatusBar *status_bar;
    QLabel *status_bar_fps;
    SuperShuckieTimestamp *status_bar_time;

    QAction *open_rom;
    QAction *close_rom;
    QAction *unload_rom;

    QAction *new_game;
    QAction *load_game;
    QAction *save_game;
    QAction *save_new_game;
    QAction *reset_console;
    QAction *reload_core;
    QAction *pause;
    QAction *quit;

    QAction *record_replay;
    QAction *resume_replay;
    QAction *play_replay;
    QAction *auto_stop_replay_on_input;
    QAction *auto_unpause_on_input;
    QAction *auto_pause_on_record;
    QAction *keyboard_replay_controls;
    QAction *horizontal_nds;
    QAction *nds_jit;
    QAction *ignore_speed_changes_in_replay;
    QAction *auto_resync_keyframes_in_replay;
    QAction *continue_last_replay;

    QAction *sgb_enabled;
    QMenu *game_boy_settings;
    NumberedAction *gbc_mode[3];

    QLabel *current_state;
    QLabel *paused_state;

    ReplayPlaybackControls *playback_bar;

    QAction *use_number_row_for_quick_slots;
    QAction *show_status_bar;
    QAction *enable_pokeabyte_integration;
    QAction *enable_external_commands;

    SuperShuckieReplayState last_known_replay_state = SuperShuckieReplayState::SuperShuckieReplayState__NoReplay;

    static const std::size_t QUICK_SAVE_STATE_COUNT = 9;

    QAction *quick_load_save_states[QUICK_SAVE_STATE_COUNT];
    QAction *quick_save_save_states[QUICK_SAVE_STATE_COUNT];

    static const std::size_t VIDEO_SCALE_COUNT = 12;

    NumberedAction *change_video_scale[VIDEO_SCALE_COUNT];

    bool use_number_keys_for_quick_slots = false;

    bool replay_time_shown = false;
    bool temporarily_paused = false;

    void set_up_file_menu();
    void set_up_gameplay_menu();
    void set_up_save_states_menu();
    void set_up_replays_menu();
    void set_up_settings_menu();

    void rebuild_recent_roms_menu() noexcept;

    void refresh_action_states();
    void set_quick_load_shortcuts();

    void quick_save(std::uint8_t index);
    void quick_load(std::uint8_t index);
    void set_gbc_mode(std::uint8_t mode);

    void make_save_state(const char *state);
    void load_save_state(const char *state);

    void set_video_scale(std::uint8_t scale);

    void closeEvent(QCloseEvent *event) override;

    bool is_game_running();

    char title_text[128] = {};

    static void on_refresh_screens(void *user_data, std::size_t screen_count, const uint32_t *const *pixels);
    static void on_change_video_mode(void *user_data, std::size_t screen_count, const SuperShuckieScreenData *screen_data, std::uint8_t scaling);

    std::uint32_t frames_in_last_second = 0;
    double current_fps = 0.0;
    clock::time_point second_start;
    void refresh_title();

    void stop_timer();
    void start_timer();
    int timer_stack = 0;

    QString app_dir;

    SDLEventWrapper sdl;

private slots:
    void do_open_rom();
    void do_close_rom();
    void do_unload_rom();
    void do_new_game() noexcept;
    void do_load_game();
    void do_save_game();
    void do_save_new_game();
    void do_reset_console();
    void do_toggle_pause();
    void do_toggle_number_row_for_save_states();
    void do_record_replay();
    void do_resume_replay();
    void do_play_replay();
    void do_open_game_speed_dialog() noexcept;
    void do_undo_load_save_state();
    void do_redo_load_save_state();
    void do_toggle_status_bar();
    void do_toggle_pokeabyte();
    void do_toggle_stop_replay_on_input();
    void do_open_controls_settings_dialog() noexcept;
    void do_toggle_auto_unpause_on_input();
    void do_toggle_auto_pause_on_record();
    void do_open_user_dir();
    void do_change_playback_time(int frames);
    void do_toggle_replay_keyboard_controls();
    void do_toggle_sgb();
    void do_open_nds_date_dialog() noexcept;
    void do_toggle_horizontal_nds();
    void do_toggle_nds_jit();
    void do_clear_recent_roms();
    void do_reload_core();
    void do_toggle_external_commands();
    void do_toggle_ignore_speed_changes_in_replay();
    void do_toggle_auto_resync_keyframes_in_replay();
    void do_continue_last_replay();
};

class NumberedAction: public QAction {
    Q_OBJECT
    friend MainWindow;
public:
    typedef void (MainWindow::*on_activated)(std::uint8_t);
    NumberedAction(MainWindow *parent, const char *text, std::uint8_t number, on_activated activated);
private:
    std::uint8_t number;
    MainWindow *parent;
    on_activated activated_fn;
private slots:
    void activated();
};

class StringAction: public QAction {
    Q_OBJECT
    friend MainWindow;
public:
    typedef void (MainWindow::*on_activated)(const char *);
    StringAction(MainWindow *parent, const char *text, const char *string, on_activated activated);
private:
    std::string string;
    MainWindow *parent;
    on_activated activated_fn;
private slots:
    void activated();
};

}

#endif
