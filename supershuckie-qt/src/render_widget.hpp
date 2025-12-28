#ifndef __SUPERSHUCKIE_RENDER_VIEW_HPP__
#define __SUPERSHUCKIE_RENDER_VIEW_HPP__

#include <QWidget>
#include <QGraphicsView>
#include <QPixmap>
#include <vector>

class QGraphicsScene;
struct SuperShuckieScreenData;

namespace SuperShuckie64 {

class MainWindow;
class SuperShuckieGraphicsView;

class GameRenderWidget: public QGraphicsView {
    friend MainWindow;
public:
    void set_dimensions(unsigned screen_count, const SuperShuckieScreenData *screen_data, unsigned scale) noexcept;

private:
    GameRenderWidget(MainWindow *window, QWidget *parent);
    MainWindow *main_window;

    struct ScreenData {
        unsigned width;
        unsigned height;
        QPixmap pixmap;
        QGraphicsPixmapItem *pixmap_item = nullptr;
        unsigned x = 0;
        unsigned y = 0;
    };

    unsigned total_width = 1, total_height = 1;

    std::vector<ScreenData> screens;
    QGraphicsScene *scene = nullptr;

    void refresh_screen(unsigned screen_count, const uint32_t *const *pixels);

    void keyPressEvent(QKeyEvent *event) override;
    void keyReleaseEvent(QKeyEvent *event) override;
    
    void dragEnterEvent(QDragEnterEvent *event) override;
    void dragMoveEvent(QDragMoveEvent *event) override;
    void dropEvent(QDropEvent *event) override;
};

}

#endif
