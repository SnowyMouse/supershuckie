#include "nds_date_dialog.hpp"
#include "main_window.hpp"

#include <QLabel>
#include <QSpinBox>
#include <QGridLayout>
#include <QPushButton>

using namespace SuperShuckie64;

NDSDateDialog::NDSDateDialog(MainWindow *main_window): QDialog(main_window), main_window(main_window) {
    this->setWindowTitle("Set Nintendo DS date");
    auto *layout = new QGridLayout(this);

    int year_row = 100;
    int month_row = 101;
    int day_row = 102;
    int hour_row = 103;
    int minute_row = 104;
    int second_row = 105;

    layout->addWidget(new QLabel("Year", this), year_row, 0);
    this->year = new QSpinBox(this);
    layout->addWidget(year, year_row, 1);

    layout->addWidget(new QLabel("Month", this), month_row, 0);
    this->month = new QSpinBox(this);
    layout->addWidget(month, month_row, 1);

    layout->addWidget(new QLabel("Day", this), day_row, 0);
    this->day = new QSpinBox(this);
    layout->addWidget(day, day_row, 1);

    layout->addWidget(new QLabel("Hour", this), hour_row, 0);
    this->hour = new QSpinBox(this);
    layout->addWidget(hour, hour_row, 1);

    layout->addWidget(new QLabel("Minute", this), minute_row, 0);
    this->minute = new QSpinBox(this);
    layout->addWidget(minute, minute_row, 1);

    layout->addWidget(new QLabel("Second", this), second_row, 0);
    this->second = new QSpinBox(this);
    layout->addWidget(second, second_row, 1);

    QLabel *note;
    bool is_nds = supershuckie_frontend_get_emulator_type(this->main_window->frontend) == SuperShuckieEmulatorType::SuperShuckieEmulatorType__NintendoDS;

    if(is_nds) {
        note = new QLabel("Notes:\n• Changes will apply upon reloading the core or loading a save.\n• Replays and save states will ignore this setting.", this);
    }
    else {
        note = new QLabel("Note: Replays and save states will ignore this setting.", this);
    }

    note->setAttribute(Qt::WA_MacSmallSize);
    layout->addWidget(note, 200, 0, 1, 2);

    auto *save = new QPushButton("Save and close", this);
    connect(save, SIGNAL(clicked()), this, SLOT(accept()));
    layout->addWidget(save, 201, 0);

    if(
        supershuckie_frontend_get_replay_state(this->main_window->frontend) != SuperShuckieReplayState::SuperShuckieReplayState__NoReplay || !is_nds
    ) {
        layout->addWidget(save, 201, 0, 1, 2);
    }
    else {
        auto *save_reload = new QPushButton("Save and reload core", this);
        connect(save_reload, SIGNAL(clicked()), this, SLOT(save_and_reload()));
        layout->addWidget(save, 201, 0);
        layout->addWidget(save_reload, 201, 1);

        save_reload->setDefault(true);
    }

    SuperShuckieNintendoDSDate date = {};
    supershuckie_frontend_get_nds_date(this->main_window->frontend, &date);

    this->year->setMinimum(2000);
    this->year->setMaximum(2099);
    this->year->setValue(date.year);

    this->month->setMinimum(1);
    this->month->setMaximum(12);
    this->month->setValue(date.month);

    this->day->setMinimum(1);
    this->day->setMaximum(31);
    this->day->setValue(date.day);

    this->hour->setMinimum(0);
    this->hour->setMaximum(23);
    this->hour->setValue(date.hour);

    this->minute->setMinimum(0);
    this->minute->setMaximum(59);
    this->minute->setValue(date.minute);

    this->second->setMinimum(0);
    this->second->setMaximum(59);
    this->second->setValue(date.second);

    this->setFixedSize(this->sizeHint());
}

int NDSDateDialog::exec() {
    this->main_window->stop_timer();
    int return_value = QDialog::exec();
    this->main_window->start_timer();
    return return_value;
}

void NDSDateDialog::accept() {
    SuperShuckieNintendoDSDate date = {};

    date.year = this->year->value();
    date.month = this->month->value();
    date.day = this->day->value();
    date.hour = this->hour->value();
    date.minute = this->minute->value();
    date.second = this->second->value();

    supershuckie_frontend_set_nds_date(this->main_window->frontend, &date);

    if(this->should_reload_core) {
        supershuckie_frontend_reload_core(this->main_window->frontend);
    }

    QDialog::accept();
}

void NDSDateDialog::save_and_reload() {
    this->should_reload_core = true;
    this->accept();
}
