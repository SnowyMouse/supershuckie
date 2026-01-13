// shared memory in macOS

#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <unistd.h>
#include <fcntl.h>
#include <errno.h>
#include <string.h>
#include <stdbool.h>

static int fd = -1;
static const char *shm = "/tmp/EDPS_MemoryData.bin";

uint8_t *supershuckie_pokeabyte_try_create_shared_memory(size_t len, const char **error) {
    if(fd != -1) {
        if(error) {
            *error = "shared memory already created";
        }
        return NULL;
    }

    // Remove the shared memory if it already exists; ftruncate only works once per shared memory.
    shm_unlink(shm);

    int new_fd = shm_open(shm, O_CREAT|O_RDWR, S_IRUSR|S_IWUSR);
    if(new_fd < 0) {
        if(error) {
            *error = "shm_open failed";
        }
        return NULL;
    }

    if(ftruncate(new_fd, len) != 0) {
        if(error) {
            *error = "ftruncate failed";
        }
        close(new_fd);
        return NULL;
    }

    uint8_t *f = mmap(NULL, len, PROT_READ | PROT_WRITE, MAP_SHARED, new_fd, 0);

    if(f == (void *)-1) {
        if(error) {
            *error = "mmap failed";
        }
        close(new_fd);
        return NULL;
    }

    fd = new_fd;

    if(error) {
        *error = "succeeded";
    }

    return f;
}

void supershuckie_pokeabyte_close_shared_memory(void) {
    if(fd == -1) {
        abort();
    }

    close(fd);
    fd = -1;
}
