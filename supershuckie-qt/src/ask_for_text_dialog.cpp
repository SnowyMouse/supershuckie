
#include "ask_for_text_dialog.hpp"
#include "main_window.hpp"

#include <QGridLayout>
#include <QLineEdit>
#include <QLabel>
#include <QPushButton>

using namespace SuperShuckie64;

AskForTextDialog::AskForTextDialog(MainWindow *parent, const QString &title, const QString &message, const QString &subtext, const QString &default_value): QDialog(parent), parent(parent) {
    this->setWindowTitle(title);

    auto *layout = new QGridLayout(this);

    QLabel *message_text = new QLabel(message, this);
    message_text->setAlignment(Qt::AlignHCenter);
    layout->addWidget(message_text, 0, 0);

    this->textbox = new QLineEdit(default_value, this);
    layout->addWidget(this->textbox, 5, 0);

    if(subtext != "") {
        QLabel *subtext_text = new QLabel(subtext, this);
        subtext_text->setAttribute(Qt::WA_MacSmallSize);
        subtext_text->setAlignment(Qt::AlignHCenter);
        layout->addWidget(subtext_text, 10, 0);
    }

    this->save = new QPushButton("OK", this);
    layout->addWidget(save, 9999, 0);

    this->setFixedSize(this->sizeHint());
    this->textbox->selectAll();

    connect(this->textbox, SIGNAL(textChanged(const QString &)), this, SLOT(check_text()));
    connect(this->save, SIGNAL(clicked()), this, SLOT(accept()));

    this->check_text();
}

void AskForTextDialog::check_text() {
    this->save->setEnabled(!this->textbox->text().isEmpty());
}

QString AskForTextDialog::text() const {
    return this->textbox->text();
}

std::optional<std::string> AskForTextDialog::ask(MainWindow *parent, const QString &title, const QString &message, const QString &subtext, const QString &default_value) {
    auto *dialog = new AskForTextDialog(parent, title, message, subtext, default_value);
    int exec_result = dialog->exec();
    auto text = dialog->text().toStdString();
    delete dialog;

    if(exec_result != QDialog::Accepted || text.empty()) {
        return std::nullopt;
    }

    return text;
}

int AskForTextDialog::exec() {
    this->parent->stop_timer();
    int return_value = QDialog::exec();
    this->parent->start_timer();
    return return_value;
}
