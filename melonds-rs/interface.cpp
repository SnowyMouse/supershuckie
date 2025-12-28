#include "melonDS/src/NDS.h"
#include "melonDS/src/Platform.h"
#include <cstdlib>
#include <memory>
#include <semaphore>

using namespace melonDS;

static u64 ms;

struct MelonDSCoreHolder {
    std::unique_ptr<NDS> nds;
};

extern "C" MelonDSCoreHolder *melonds_rs_core_new(
    const u8 *rom,
    std::size_t rom_size,
    const u8 *sram,
    std::size_t sram_size
) {
    NDSCart::NDSCartArgs cartargs;

    auto file_data = std::make_unique<u8[]>(rom_size);
    std::memcpy(file_data.get(), rom, rom_size);

    auto *holder = new MelonDSCoreHolder();
    holder->nds = std::make_unique<NDS>();
    static_cast<SoftRenderer &>(holder->nds->GetRenderer3D()).SetThreaded(true, holder->nds->GPU);
    auto cart = NDSCart::ParseROM(std::move(file_data), rom_size, holder, std::move(cartargs));
    if(!cart) {
        delete holder;
        return nullptr;
    }

    holder->nds->SetNDSCart(std::move(cart));

    if(sram_size > 0) {
        holder->nds->SetNDSSave(sram, sram_size);
    }

    holder->nds->SetARM7BIOS(bios_arm7_bin);
    holder->nds->SetARM9BIOS(bios_arm9_bin);
    holder->nds->LoadBIOS();
    holder->nds->SetupDirectBoot("nds.rom");
    holder->nds->Start();

    return holder;
}

extern "C" void melonds_rs_core_free(MelonDSCoreHolder *core) {
    delete core;
}

extern "C" void melonds_rs_core_run_frame(MelonDSCoreHolder *core) {
    core->nds->RunFrame();
}

extern "C" u8 *melonds_rs_core_get_sram(const MelonDSCoreHolder *core, size_t &size) {
    size = core->nds->GetNDSSaveLength();
    return core->nds->GetNDSSave();
}

extern "C" std::size_t melonds_rs_core_create_save_state(const MelonDSCoreHolder *core, void *data, std::size_t data_size) {
    Savestate state(data, data_size, true);
    bool success = core->nds->DoSavestate(&state);
    state.Finish();

    if(!success) {
        return 0;
    }

    return state.Length();
}

extern "C" bool melonds_rs_core_load_save_state(const MelonDSCoreHolder *core, void *data, std::size_t data_size) {
    Savestate state(data, data_size, false);
    return core->nds->DoSavestate(&state);
}

extern "C" void melonds_rs_core_reset(MelonDSCoreHolder *core) {
    core->nds->Reset();
    core->nds->LoadBIOS();
    core->nds->SetupDirectBoot("nds.rom");
    core->nds->Start();
}

extern "C" u32 *melonds_rs_core_get_pixels(MelonDSCoreHolder *core, std::size_t screen) {
    // TODO: determine if and when this is necessary!
    // static_cast<SoftRenderer &>(core->nds->GetRenderer3D()).StopRenderThread();
    auto &gpu = core->nds->GPU;
    return gpu.Framebuffer[gpu.FrontBuffer][screen].get();
}

extern "C" void melonds_rs_core_set_input(MelonDSCoreHolder *core, std::uint32_t input) {
    std::uint32_t mask = 0b0000011111111111;

    auto buttons = input & mask;
    core->nds->SetKeyMask(~buttons & 0xFFF);

    if(input & 0x1000) {
        std::uint16_t y = (input >> 24) & 0xFF;
        std::uint16_t x = (input >> 16) & 0xFF;
        core->nds->TouchScreen(x, y);
    }
    else {
        core->nds->ReleaseScreen();
    }
}

extern "C" u8 *melonds_rs_core_get_ram(MelonDSCoreHolder *core) {
    return core->nds->MainRAM;
}

namespace melonDS::Platform {
    void SignalStop(StopReason reason, void* userdata) {}
    std::string GetLocalFilePath(const std::string& filename) { return std::string(); }
    FileHandle* OpenFile(const std::string& path, FileMode mode) { return nullptr; }
    FileHandle* OpenLocalFile(const std::string& path, FileMode mode) { return nullptr; }
    bool FileExists(const std::string& name) { return false; }
    bool LocalFileExists(const std::string& name) { return false; }
    bool CheckFileWritable(const std::string& filepath) { return false; }
    bool CheckLocalFileWritable(const std::string& filepath) { return false; }
    bool CloseFile(FileHandle* file) { return false; }
    bool IsEndOfFile(FileHandle* file) { return false; }
    bool FileReadLine(char* str, int count, FileHandle* file) { return false; }
    u64 FilePosition(FileHandle* file) { return 0; }
    bool FileSeek(FileHandle* file, s64 offset, FileSeekOrigin origin) { return false; }
    void FileRewind(FileHandle* file) {}
    u64 FileRead(void* data, u64 size, u64 count, FileHandle* file) { return 0; }
    bool FileFlush(FileHandle* file) { return false; }
    u64 FileWrite(const void* data, u64 size, u64 count, FileHandle* file) { return 0; }
    u64 FileWriteFormatted(FileHandle* file, const char* fmt, ...) { return 0; }
    u64 FileLength(FileHandle* file) { return 0; }
    void Log(LogLevel level, const char* fmt, ...) {
//        va_list args;
//        va_start(args, fmt);
//        std::printf("LOG (%d) ", level);
//        std::vprintf(fmt, args);
//        va_end(args);
    }

    struct Thread {
        std::thread thread;
        bool done = false;
    };

    Thread* Thread_Create(std::function<void()> func) {
        Thread *thread = new Thread();
        thread->thread = std::thread(func);
        return thread;
    }
    void Thread_Free(Thread* thread) { delete thread; }
    void Thread_Wait(Thread* thread) {
        if(!thread->done) {
            thread->done = true;
            thread->thread.join();
        }
    }

    struct Semaphore {
        std::counting_semaphore<256> semaphore;
        Semaphore(): semaphore(0) {}
    };

    Semaphore* Semaphore_Create() {
        return new Semaphore();
    }
    void Semaphore_Free(Semaphore* sema) {
        delete sema;
    }
    void Semaphore_Reset(Semaphore* sema) {
        while(Semaphore_TryWait(sema, 0)) {}
    }
    void Semaphore_Wait(Semaphore* sema) {
        sema->semaphore.acquire();
    }
    bool Semaphore_TryWait(Semaphore* sema, int timeout_ms) {
        if(timeout_ms == 0) {
            return sema->semaphore.try_acquire();
        }
        return sema->semaphore.try_acquire_for(std::chrono::milliseconds(timeout_ms));
    }
    void Semaphore_Post(Semaphore* sema, int count) {
        sema->semaphore.release(count);
    }

    struct Mutex {
        std::mutex mutex;
    };

    Mutex* Mutex_Create() { return new Mutex(); }
    void Mutex_Free(Mutex* mutex) { delete mutex; }
    void Mutex_Lock(Mutex* mutex) {
        mutex->mutex.lock();
    }
    void Mutex_Unlock(Mutex* mutex) {
        mutex->mutex.unlock();
    }
    bool Mutex_TryLock(Mutex* mutex) {
        return mutex->mutex.try_lock();
    }

    void Sleep(u64 usecs) { }
    u64 GetMSCount() { return 0; }
    u64 GetUSCount() { return 0; }


    void WriteNDSSave(const u8* savedata, u32 savelen, u32 writeoffset, u32 writelen, void* userdata) {}
    void WriteGBASave(const u8* savedata, u32 savelen, u32 writeoffset, u32 writelen, void* userdata) {}
    void WriteFirmware(const Firmware& firmware, u32 writeoffset, u32 writelen, void* userdata) {}
    void WriteDateTime(int year, int month, int day, int hour, int minute, int second, void* userdata) {}
    void MP_Begin(void* userdata) {}
    void MP_End(void* userdata) {}
    int MP_SendPacket(u8* data, int len, u64 timestamp, void* userdata) { return 0; }
    int MP_RecvPacket(u8* data, u64* timestamp, void* userdata) { return 0; }
    int MP_SendCmd(u8* data, int len, u64 timestamp, void* userdata) { return 0; }
    int MP_SendReply(u8* data, int len, u64 timestamp, u16 aid, void* userdata) { return 0; }
    int MP_SendAck(u8* data, int len, u64 timestamp, void* userdata) { return 0; }
    int MP_RecvHostPacket(u8* data, u64* timestamp, void* userdata) { return 0; }
    u16 MP_RecvReplies(u8* data, u64 timestamp, u16 aidmask, void* userdata) { return 0; }
    int Net_SendPacket(u8* data, int len, void* userdata) { return 0; }
    int Net_RecvPacket(u8* data, void* userdata) { return 0; }
    void Camera_Start(int num, void* userdata) {}
    void Camera_Stop(int num, void* userdata) {}
    void Camera_CaptureFrame(int num, u32* frame, int width, int height, bool yuv, void* userdata) {}
    void Mic_Start(void* userdata) {}
    void Mic_Stop(void* userdata) {}
    int Mic_ReadInput(s16* data, int maxlength, void* userdata) { return 0; }
    struct AACDecoder {};
    AACDecoder* AAC_Init() { return new AACDecoder(); }
    void AAC_DeInit(AACDecoder* dec) { delete dec; }
    bool AAC_Configure(AACDecoder* dec, int frequency, int channels) { return true; }
    bool AAC_DecodeFrame(AACDecoder* dec, const void* input, int inputlen, void* output, int outputlen) { return true; }
    bool Addon_KeyDown(KeyType type, void* userdata) { return true; }
    void Addon_RumbleStart(u32 len, void* userdata) {}
    void Addon_RumbleStop(void* userdata) {}
    float Addon_MotionQuery(MotionQueryType type, void* userdata) { return 0.0; }
}
