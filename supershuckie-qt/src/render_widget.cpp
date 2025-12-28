#include "render_widget.hpp"
#include "main_window.hpp"
#include <QGraphicsPixmapItem>
#include <QKeyEvent>
#include <QMimeData>

using namespace SuperShuckie64;

GameRenderWidget::GameRenderWidget(MainWindow *window, QWidget *parent): QGraphicsView(parent), main_window(window) {
    this->setFrameStyle(0);
    this->setHorizontalScrollBarPolicy(Qt::ScrollBarPolicy::ScrollBarAlwaysOff);
    this->setVerticalScrollBarPolicy(Qt::ScrollBarPolicy::ScrollBarAlwaysOff);
    this->setSizePolicy(QSizePolicy::Policy::Fixed, QSizePolicy::Policy::Fixed);
    this->setFocusPolicy(Qt::ClickFocus);
}

void GameRenderWidget::set_dimensions(unsigned screen_count, const SuperShuckieScreenData *screen_data, unsigned scale) noexcept {
    if(scale == 0) {
        scale = 1;
    }

    this->current_scale = scale;
    this->scale(scale, scale);
    this->setTransform(QTransform::fromScale(scale, scale));

    delete this->scene;
    this->scene = nullptr;

    this->screens.clear();

    this->scene = new QGraphicsScene(this);

    this->total_width = 0;
    this->total_height = 0;

    bool horizontal_nds = this->main_window->horizontal_nds->isChecked();

    for(unsigned i = 0; i < screen_count; i++) {
        ScreenData screen;
        screen.width = screen_data[i].width;
        screen.height = screen_data[i].height;
        screen.pixmap_item = this->scene->addPixmap(screen.pixmap);

        if(horizontal_nds) {
            screen.x = this->total_width;
            this->total_width += screen.width;
            this->total_height = this->total_height > screen.height ? this->total_height : screen.height;
        }
        else {
            screen.y = this->total_height;
            this->total_height += screen.height;
            this->total_width = this->total_width > screen.width ? this->total_width : screen.width;
        }

        screen.pixmap_item->setOffset(screen.x, screen.y);
        this->screens.emplace_back(screen);
    }
    
    this->setFixedSize(this->total_width * scale, this->total_height * scale);
    this->setScene(this->scene);
}

void GameRenderWidget::refresh_screen(unsigned screen_count, const uint32_t *const *pixels) {
    for(unsigned i = 0; i < this->screens.size() && i < screen_count; i++) {
        auto &screen = this->screens[i];
        screen.pixmap.convertFromImage(QImage(reinterpret_cast<const uchar *>(pixels[i]), screen.width, screen.height, QImage::Format::Format_ARGB32));
        screen.pixmap_item->setPixmap(screen.pixmap);
    }
}

void GameRenderWidget::keyPressEvent(QKeyEvent *event) {
    QWidget::keyPressEvent(event);
    
    if(this->main_window->frontend != nullptr) {
        // Replay controls
        int key = event->key();
        bool auto_repeat = event->isAutoRepeat();
        bool is_paused = supershuckie_frontend_is_paused(this->main_window->frontend);

        if(
            this->main_window->keyboard_replay_controls->isChecked() && 
            supershuckie_frontend_get_replay_state(this->main_window->frontend) == SuperShuckieReplayState::SuperShuckieReplayState__Playback
        ) {
            switch(key) {
                case Qt::Key_Space:
                    if(!auto_repeat) {
                        supershuckie_frontend_set_paused(
                            this->main_window->frontend,
                            !supershuckie_frontend_is_paused(this->main_window->frontend)
                        );
                    }
                    return;
                case Qt::Key_Left:
                    supershuckie_frontend_advance_playback_frames(
                        this->main_window->frontend,
                        -240
                    );
                    return;
                case Qt::Key_Right:
                    supershuckie_frontend_advance_playback_frames(
                        this->main_window->frontend,
                        240
                    );
                    return;
                case Qt::Key_Up:
                case Qt::Key_Down:
                    // up/down arrow keys do not do anything yet
                    return;
                case Qt::Key_Comma:
                    if(is_paused) {
                        supershuckie_frontend_advance_playback_frames(
                            this->main_window->frontend,
                            -1
                        );
                    }
                    return;
                case Qt::Key_Period:
                    if(is_paused) {
                        supershuckie_frontend_advance_playback_frames(
                            this->main_window->frontend,
                            1
                        );
                    }
                    return;
            }
        }

        if(!auto_repeat) {
            supershuckie_frontend_key_press(this->main_window->frontend, key, true);
        }
    }
}

void GameRenderWidget::keyReleaseEvent(QKeyEvent *event) {
    QWidget::keyReleaseEvent(event);

    if(this->main_window->frontend != nullptr && !event->isAutoRepeat()) {
        supershuckie_frontend_key_press(this->main_window->frontend, event->key(), false);
    }
}


template<typename T> static std::optional<std::filesystem::path> validate_event(T *event) {
    auto *d = event->mimeData();
    if(d->hasUrls()) {
        auto urls = d->urls();
        if(urls.length() == 1) {
            auto path = std::filesystem::path(urls[0].toLocalFile().toStdString());
            return path;
        }
    }
    return std::nullopt;
}

void GameRenderWidget::dragEnterEvent(QDragEnterEvent *event) {
    if(validate_event(event)) {
        event->accept();
    }
}

void GameRenderWidget::dragMoveEvent(QDragMoveEvent *event) {
    if(validate_event(event)) {
        event->accept();
    }
}

void GameRenderWidget::dropEvent(QDropEvent *event) {
    auto path = validate_event(event);
    if(path) {
        this->main_window->load_rom(*path);
    }
}

void GameRenderWidget::mousePressEvent(QMouseEvent *event) {
    auto pos = event->position();
    int x = pos.x() / this->current_scale;
    int y = pos.y() / this->current_scale;

    bool horizontal_nds = this->main_window->horizontal_nds->isChecked();

    if(this->screens.size() == 2) {
        auto &screen = this->screens[1];

        if(horizontal_nds) {
            x -= screen.x;
        }
        else {
            y -= screen.y;
        }

        if(x < 0 || x > screen.width || y < 0 || y > screen.height) {
            return;
        }
        supershuckie_frontend_set_touch(this->main_window->frontend, true, x, y);
    }
}

void GameRenderWidget::mouseDoubleClickEvent(QMouseEvent *event) {
    this->mousePressEvent(event);
}

void GameRenderWidget::mouseReleaseEvent(QMouseEvent *) {
    supershuckie_frontend_set_touch(this->main_window->frontend, false, 0, 0);
}

void GameRenderWidget::mouseMoveEvent(QMouseEvent *event) {
    this->mousePressEvent(event);
}