#include <cstdint>
#include <cstddef>
#include <exception>
#include <cstdio>
#include <vector>

#define ENABLE_VFS
#define ENABLE_DIRECTORIES

#include <mgba/core/core.h>
#include <mgba/core/log.h>
#include <mgba-util/vfs.h>
#include <mgba/gba/core.h>

static void nope_log(struct mLogger*, int category, enum mLogLevel level, const char* format, va_list args) {
}

static mLogger logger = {
    .log = nope_log
};


struct MGBACoreRaw {
    mCore *core;

    VFile *rom_vf;
    std::vector<std::byte> rom;

    VFile *sram_vf;
    std::vector<std::byte> sram;

    VFile *bios_vf;
    std::vector<std::byte> bios;

    std::vector<std::uint32_t> pixels;

    std::size_t iwram = ~0;
    std::size_t ewram = ~0;
};

extern "C" MGBACoreRaw *mgba_rs_core_new(
    const std::byte *rom,
    std::size_t rom_size,
    const std::byte *sram,
    std::size_t sram_size,
    const std::byte *bios,
    std::size_t bios_size
) {
    mLogSetDefaultLogger(&logger);

    auto *core = new MGBACoreRaw();
    core->core = mCoreCreate(mPLATFORM_GBA);

    if(core->core == nullptr) {
        std::printf("Failed to create mGBA instance\n");
        std::terminate();
    }

	mCoreInitConfig(core->core, nullptr);

	core->core->opts.skipBios = true;

    if(!core->core->init(core->core)) {
        std::printf("Failed to init mGBA\n");
        std::terminate();
    }

    core->rom = std::vector(rom, rom + rom_size);
    core->rom_vf = VFileFromMemory(core->rom.data(), core->rom.size());
    core->core->loadROM(core->core, core->rom_vf);

    if(sram_size > 0) {
        core->sram = std::vector(sram, sram + sram_size);
        core->sram_vf = VFileFromMemory(core->sram.data(), core->sram.size());
        core->core->loadSave(core->core, core->sram_vf);
    }

    const mCoreMemoryBlock *blocks;
    std::size_t block_count = core->core->listMemoryBlocks(core->core, &blocks);

    for(std::size_t i = 0; i < block_count; i++) {
        if(blocks[i].start == 0x2000000) {
            core->ewram = blocks[i].id;
        }
        if(blocks[i].start == 0x3000000) {
            core->iwram = blocks[i].id;
        }
    }

    if(core->ewram == ~0) {
        std::printf("Failed to find ewram\n");
        std::terminate();
    }

    if(core->iwram == ~0) {
        std::printf("Failed to find iwram\n");
        std::terminate();
    }

    core->pixels.resize(240 * 160);
    core->core->setVideoBuffer(core->core, core->pixels.data(), 240);

    if(bios_size > 0) {
        core->bios = std::vector(bios, bios + bios_size);
        core->bios_vf = VFileFromMemory(core->bios.data(), core->bios.size());
        if(!core->core->loadBIOS(core->core, core->bios_vf, 0)) {
            std::printf("Bad BIOS\n");
            std::terminate();
        }
    }

    core->core->reset(core->core);

    return core;
}

extern "C" void mgba_rs_core_free(MGBACoreRaw *core) {
    if(core == nullptr) {
        return;
    }

    core->core->deinit(core->core);

    delete core;
}

extern "C" void mgba_rs_core_run_frame(MGBACoreRaw *core) {
    core->core->runFrame(core->core);
}

extern "C" void mgba_rs_core_reset(MGBACoreRaw *core) {
    core->core->reset(core->core);
}

extern "C" const std::uint32_t *mgba_rs_core_get_pixels(const MGBACoreRaw *core) {
    return core->pixels.data();
}

extern "C" void mgba_rs_core_set_input(MGBACoreRaw *core, std::uint16_t input) {
    core->core->setKeys(core->core, input);
}

extern "C" const void *mgba_rs_core_get_sram(const MGBACoreRaw *core, std::size_t &size) {
    void *sram = nullptr;
    size = core->core->savedataClone(core->core, &sram);
    return sram;
}

extern "C" std::size_t mgba_rs_core_create_save_state(const MGBACoreRaw *core, std::byte *data, std::size_t data_size) {
    auto size = core->core->stateSize(core->core);
    if(size <= data_size) {
        core->core->saveState(core->core, data);
    }
    return size;
}

extern "C" bool mgba_rs_core_load_save_state(MGBACoreRaw *core, const std::byte *data, std::size_t data_size) {
    if(core->core->stateSize(core->core) != data_size) {
        return false;
    }
    core->core->loadState(core->core, data);
    return true;
}

extern "C" void *mgba_rs_core_get_ewram(MGBACoreRaw *core) {
    std::size_t q;
    auto *ptr = core->core->getMemoryBlock(core->core, core->ewram, &q);
    if(ptr != nullptr) {
        return ptr;
    }
    std::printf("Failed to get ewram\n");
    std::terminate();
}

extern "C" void *mgba_rs_core_get_iwram(MGBACoreRaw *core) {
    std::size_t q;
    auto *ptr = core->core->getMemoryBlock(core->core, core->iwram, &q);
    if(ptr != nullptr) {
        return ptr;
    }
    std::printf("Failed to get iwram\n");
    std::terminate();
}
