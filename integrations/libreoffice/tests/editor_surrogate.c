/* Test-only external editor: atomically save a fixture, await acceptance, exit.
 * Put renderer-path.txt and edited.rsk beside the executable. Worker arguments
 * delegate to the real renderer. Never use this as a user's configured editor.
 */
#include <stdio.h>
#include <string.h>
#ifdef _WIN32
#include <process.h>
#include <windows.h>
#else
#include <unistd.h>
#endif

static unsigned long move_error = 0;

static int copy_file(const char *from, const char *to) {
    FILE *input = fopen(from, "rb"), *output;
    char buffer[8192];
    size_t count;
    if (!input) return 1;
    output = fopen(to, "wb");
    if (!output) { fclose(input); return 2; }
    while ((count = fread(buffer, 1, sizeof buffer, input)) > 0) {
        if (fwrite(buffer, 1, count, output) != count) {
            fclose(input); fclose(output); return 3;
        }
    }
    fclose(input);
    return fclose(output);
}

static char *filename(char *path) {
    char *slash = strrchr(path, '/'), *backslash = strrchr(path, '\\');
    if (backslash && (!slash || backslash > slash)) slash = backslash;
    return slash ? slash + 1 : path;
}

static int run(int argc, char **argv) {
    char base[8192], path[8192], program[8192], temp[8192], accepted[8192];
    FILE *file;
    if (strlen(argv[0]) >= sizeof base - 64) return 10;
    strcpy(base, argv[0]);
    *filename(base) = '\0';
    snprintf(path, sizeof path, "%srenderer-path.txt", base);
    file = fopen(path, "r");
    if (!file || !fgets(program, sizeof program, file)) return 11;
    fclose(file);
    program[strcspn(program, "\r\n")] = '\0';
    if (argc < 3 || strcmp(argv[1], "--open")) {
        argv[0] = program;
#ifdef _WIN32
        return (int)_spawnv(_P_WAIT, program, (const char *const *)argv);
#else
        execv(program, argv);
        return 12;
#endif
    }
    if (strlen(argv[2]) >= sizeof accepted - 64) return 13;
    snprintf(path, sizeof path, "%sedited.rsk", base);
    snprintf(temp, sizeof temp, "%s.new", argv[2]);
    if (copy_file(path, temp)) return 14;
#ifdef _WIN32
    int moved = 0;
    for (int i = 0; i < 200; i++) {
        if (MoveFileExA(temp, argv[2], MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)) {
            moved = 1;
            break;
        }
        move_error = GetLastError();
        if (move_error != ERROR_SHARING_VIOLATION && move_error != ERROR_ACCESS_DENIED) break;
        Sleep(10);
    }
    if (!moved) return 15;
#else
    if (rename(temp, argv[2])) return 15;
#endif
    strcpy(accepted, argv[2]);
    strcpy(filename(accepted), "accepted.json");
    for (int i = 0; i < 300; i++) {
        file = fopen(accepted, "rb");
        if (file) { fclose(file); break; }
#ifdef _WIN32
        Sleep(100);
#else
        usleep(100000);
#endif
    }
    snprintf(path, sizeof path, "%ssession-path.txt", base);
    file = fopen(path, "w");
    if (!file) return 16;
    fputs(argv[2], file);
    fclose(file);
    snprintf(path, sizeof path, "%saccepted-receipt.json", base);
    return copy_file(accepted, path);
}

int main(int argc, char **argv) {
    char path[8192];
    int editing = argc >= 3 && !strcmp(argv[1], "--open");
    if (strlen(argv[0]) >= sizeof path - 64) return 10;
    strcpy(path, argv[0]);
    strcpy(filename(path), "editor-exit.txt");
    int result = run(argc, argv);
    if (editing) {
        FILE *file = fopen(path, "w");
        if (file) { fprintf(file, "%d move_error=%lu\n", result, move_error); fclose(file); }
    }
    return result;
}
