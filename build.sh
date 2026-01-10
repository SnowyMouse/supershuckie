#!/bin/sh

git pull --recurse-submodules=yes

BUILD_MODE=Release

cmake ./melonds-rs/melonDS -B build/melonDS \
	-DENABLE_JIT=ON \
	-DENABLE_OGLRENDERER=OFF \
	-DENABLE_GDBSTUB=OFF \
	-DBUILD_QT_SDL=OFF \
	-DCMAKE_BUILD_TYPE=$BUILD_MODE
make -C build/melonDS -j$(nproc)

cmake ./mgba-rs/mgba -B build/mgba \
	-DLIBMGBA_ONLY=ON \
	-DDISABLE_FRONTENDS=ON \
	-DCMAKE_BUILD_TYPE=$BUILD_MODE
make -C build/mgba -j$(nproc)

cmake ./supershuckie-qt  -B build -DCMAKE_BUILD_TYPE=$BUILD_MODE -DSCRIPT_BUILD=ON
make -C build -j$(nproc)
