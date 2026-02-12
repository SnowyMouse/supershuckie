
#include "select_item_dialog.hpp"
#include "main_window.hpp"

#include <QGridLayout>
#include <QListWidget>
#include <QLabel>
#include <QPushButton>

using namespace SuperShuckie64;

SelectItemDialog::SelectItemDialog(MainWindow *parent, std::vector<std::string> items, const QString &title, const QString &message, const QString &subtext): QDialog(parent), parent(parent) {
    this->setWindowTitle(title);

    auto *layout = new QGridLayout(this);

    QLabel *message_text = new QLabel(message, this);
    message_text->setAlignment(Qt::AlignHCenter);
    layout->addWidget(message_text, 0, 0);

    this->list = new QListWidget(this);

    for(auto &item : items) {
        this->list->addItem(item.c_str());
    }

    this->list->connect(this->list, SIGNAL(itemActivated(QListWidgetItem *)), this, SLOT(accept()));

    layout->addWidget(this->list, 5, 0);

    if(subtext != "") {
        QLabel *subtext_text = new QLabel(subtext, this);
        subtext_text->setAttribute(Qt::WA_MacSmallSize);
        subtext_text->setAlignment(Qt::AlignHCenter);
        layout->addWidget(subtext_text, 10, 0);
    }

    auto *save = new QPushButton("OK", this);
    connect(save, SIGNAL(clicked()), this, SLOT(accept()));
    layout->addWidget(save, 9999, 0);

    this->setFixedSize(this->sizeHint());
}

QString SelectItemDialog::text() const {
    auto *item = this->list->currentItem();
    if(item == nullptr) {
        return "";
    }
    return item->text();
}

std::optional<std::string> SelectItemDialog::ask(MainWindow *parent, std::vector<std::string> items, const QString &title, const QString &message, const QString &subtext) {
    auto *dialog = new SelectItemDialog(parent, items, title, message, subtext);
    int exec_result = dialog->exec();
    auto text = dialog->text().toStdString();
    delete dialog;

    if(exec_result != QDialog::Accepted || text.empty()) {
        return std::nullopt;
    }

    return text;
}

int SelectItemDialog::exec() {
    this->parent->stop_timer();
    int return_value = QDialog::exec();
    this->parent->start_timer();
    return return_value;
}
