#ifndef __SUPERSHUCKIE_NDS_DATE_DIALOG_HPP__
#define __SUPERSHUCKIE_NDS_DATE_DIALOG_HPP__

#include <QDialog>

class QSpinBox;

namespace SuperShuckie64 {

class MainWindow;

class NDSDateDialog: public QDialog {
    Q_OBJECT
    friend MainWindow;

public:
    NDSDateDialog(MainWindow *main_window);
    int exec() override;

private:
    MainWindow *main_window;

    QSpinBox *year;
    QSpinBox *month;
    QSpinBox *day;
    QSpinBox *hour;
    QSpinBox *minute;
    QSpinBox *second;

    bool should_reload_core = false;

    void accept() override;

private slots:
    void save_and_reload();
};

}

#endif
