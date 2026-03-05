#include <stdio.h>

#include <SDL3/SDL.h>
#include <QApplication>

#ifdef _WIN32
#include <QStyleFactory>
#endif

#include "main_window.hpp"
#include "theme.hpp"

int main(int argc, char **argv) {
    SDL_Init(SDL_INIT_AUDIO | SDL_INIT_EVENTS | SDL_INIT_GAMEPAD | SDL_INIT_VIDEO);

    QCoreApplication::setOrganizationName("SnowyMouse");
    QCoreApplication::setApplicationName("SuperShuckie");

    QApplication app(argc, argv);

    SixShooter::Theme theme;

    SuperShuckie64::MainWindow window;
    window.show();

    if(argc == 2) {
        window.load_rom(argv[1]);
    }

    int result = app.exec();
    SDL_Quit();

    return result;
}
