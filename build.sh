#!/bin/sh

git pull --recurse-submodules=yes
cmake ./melonds-rs/melonDS -B build/melonDS \
	-DENABLE_JIT=OFF \
	-DENABLE_OGLRENDERER=OFF \
	-DENABLE_GDBSTUB=OFF \
	-DBUILD_QT_SDL=OFF \
	-DCMAKE_BUILD_TYPE=Release 
make -C build/melonDS -j$(nproc)

cmake ./supershuckie-qt  -B build -DCMAKE_BUILD_TYPE=Release -DSCRIPT_BUILD=ON
make -C build -j$(nproc)