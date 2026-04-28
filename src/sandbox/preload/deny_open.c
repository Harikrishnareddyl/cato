/*
 * deny_open.c — LD_PRELOAD library for Cato sandbox (Linux)
 *
 * Intercepts open/openat/fopen and blocks access to files matching
 * deny_read/deny_write patterns. Patterns are passed via environment
 * variables:
 *   CATO_DENY_READ=*.env,*.key,*.pem,id_rsa
 *   CATO_DENY_WRITE=*.lock
 *
 * This is a safety net, not a security boundary. Processes using raw
 * syscalls (not libc) can bypass this. The bwrap bind mounts handle
 * existing files; this catches NEW files created during the session.
 *
 * Compile: gcc -shared -fPIC -o libcato_deny.so deny_open.c -ldl
 */

#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

/* Max patterns and pattern length */
#define MAX_PATTERNS 64
#define MAX_PATTERN_LEN 256

static char deny_read_patterns[MAX_PATTERNS][MAX_PATTERN_LEN];
static int deny_read_count = 0;
static char deny_write_patterns[MAX_PATTERNS][MAX_PATTERN_LEN];
static int deny_write_count = 0;
static int initialized = 0;

/* Original function pointers */
typedef int (*orig_open_t)(const char *, int, ...);
typedef int (*orig_openat_t)(int, const char *, int, ...);
typedef FILE *(*orig_fopen_t)(const char *, const char *);

/* Extract basename from path */
static const char *get_basename(const char *path) {
    const char *base = strrchr(path, '/');
    return base ? base + 1 : path;
}

/* Check if filename matches a glob pattern */
static int matches_pattern(const char *filename, const char *pattern) {
    /* *.ext — extension match */
    if (pattern[0] == '*' && pattern[1] == '.') {
        const char *ext = pattern + 1; /* .env, .key, etc */
        size_t ext_len = strlen(ext);
        size_t name_len = strlen(filename);
        if (name_len >= ext_len && strcmp(filename + name_len - ext_len, ext) == 0)
            return 1;
        /* Also check for *.env.* pattern: e.g., *.env.* matches .env.local */
        /* If pattern is like *.env.* check if filename contains .env. */
        if (ext_len > 1 && ext[ext_len - 1] == '*' && ext[ext_len - 2] == '.') {
            char mid[MAX_PATTERN_LEN];
            strncpy(mid, ext, ext_len - 1);
            mid[ext_len - 1] = '\0';
            if (strstr(filename, mid) != NULL)
                return 1;
        }
        return 0;
    }

    /* *word* — contains match */
    if (pattern[0] == '*' && pattern[strlen(pattern) - 1] == '*') {
        char word[MAX_PATTERN_LEN];
        size_t plen = strlen(pattern);
        if (plen < 3) return 0;
        strncpy(word, pattern + 1, plen - 2);
        word[plen - 2] = '\0';
        return strstr(filename, word) != NULL;
    }

    /* exact match */
    return strcmp(filename, pattern) == 0;
}

/* Parse comma-separated patterns from env var */
static int parse_patterns(const char *env_name, char patterns[][MAX_PATTERN_LEN]) {
    const char *val = getenv(env_name);
    if (!val || !val[0]) return 0;

    int count = 0;
    const char *start = val;
    while (*start && count < MAX_PATTERNS) {
        const char *comma = strchr(start, ',');
        size_t len;
        if (comma) {
            len = comma - start;
        } else {
            len = strlen(start);
        }
        if (len > 0 && len < MAX_PATTERN_LEN) {
            strncpy(patterns[count], start, len);
            patterns[count][len] = '\0';
            count++;
        }
        if (comma) {
            start = comma + 1;
        } else {
            break;
        }
    }
    return count;
}

/* Initialize patterns (once) */
static void init_patterns(void) {
    if (initialized) return;
    initialized = 1;
    deny_read_count = parse_patterns("CATO_DENY_READ", deny_read_patterns);
    deny_write_count = parse_patterns("CATO_DENY_WRITE", deny_write_patterns);
}

/* Check if path should be denied for reading */
static int is_read_denied(const char *path) {
    if (!path || deny_read_count == 0) return 0;
    const char *base = get_basename(path);
    for (int i = 0; i < deny_read_count; i++) {
        if (matches_pattern(base, deny_read_patterns[i]))
            return 1;
    }
    return 0;
}

/* Check if path should be denied for writing */
static int is_write_denied(const char *path) {
    if (!path || deny_write_count == 0) return 0;
    const char *base = get_basename(path);
    for (int i = 0; i < deny_write_count; i++) {
        if (matches_pattern(base, deny_write_patterns[i]))
            return 1;
    }
    return 0;
}

/* Check if flags indicate write intent */
static int is_write_flags(int flags) {
    return (flags & O_WRONLY) || (flags & O_RDWR) || (flags & O_CREAT) || (flags & O_TRUNC);
}

/* Intercepted open() */
int open(const char *path, int flags, ...) {
    init_patterns();

    if (is_read_denied(path) && !(is_write_flags(flags))) {
        errno = EACCES;
        return -1;
    }
    if (is_write_denied(path) && is_write_flags(flags)) {
        errno = EACCES;
        return -1;
    }
    /* Also deny read if write flags are set and path matches read deny */
    if (is_read_denied(path) && is_write_flags(flags)) {
        errno = EACCES;
        return -1;
    }

    orig_open_t orig = (orig_open_t)dlsym(RTLD_NEXT, "open");
    if (flags & O_CREAT) {
        va_list args;
        va_start(args, flags);
        mode_t mode = va_arg(args, mode_t);
        va_end(args);
        return orig(path, flags, mode);
    }
    return orig(path, flags);
}

/* Intercepted open64() */
int open64(const char *path, int flags, ...) {
    init_patterns();

    if (is_read_denied(path) && !is_write_flags(flags)) {
        errno = EACCES;
        return -1;
    }
    if (is_write_denied(path) && is_write_flags(flags)) {
        errno = EACCES;
        return -1;
    }
    if (is_read_denied(path) && is_write_flags(flags)) {
        errno = EACCES;
        return -1;
    }

    orig_open_t orig = (orig_open_t)dlsym(RTLD_NEXT, "open64");
    if (flags & O_CREAT) {
        va_list args;
        va_start(args, flags);
        mode_t mode = va_arg(args, mode_t);
        va_end(args);
        return orig(path, flags, mode);
    }
    return orig(path, flags);
}

/* Intercepted openat() */
int openat(int dirfd, const char *path, int flags, ...) {
    init_patterns();

    if (is_read_denied(path) && !is_write_flags(flags)) {
        errno = EACCES;
        return -1;
    }
    if (is_write_denied(path) && is_write_flags(flags)) {
        errno = EACCES;
        return -1;
    }
    if (is_read_denied(path) && is_write_flags(flags)) {
        errno = EACCES;
        return -1;
    }

    orig_openat_t orig = (orig_openat_t)dlsym(RTLD_NEXT, "openat");
    if (flags & O_CREAT) {
        va_list args;
        va_start(args, flags);
        mode_t mode = va_arg(args, mode_t);
        va_end(args);
        return orig(dirfd, path, flags, mode);
    }
    return orig(dirfd, path, flags);
}

/* Intercepted fopen() */
FILE *fopen(const char *path, const char *mode) {
    init_patterns();

    int writing = (strchr(mode, 'w') || strchr(mode, 'a') || strchr(mode, '+'));

    if (is_read_denied(path) && !writing) {
        errno = EACCES;
        return NULL;
    }
    if (is_write_denied(path) && writing) {
        errno = EACCES;
        return NULL;
    }
    if (is_read_denied(path) && writing) {
        errno = EACCES;
        return NULL;
    }

    orig_fopen_t orig = (orig_fopen_t)dlsym(RTLD_NEXT, "fopen");
    return orig(path, mode);
}

/* Intercepted fopen64() */
FILE *fopen64(const char *path, const char *mode) {
    init_patterns();

    int writing = (strchr(mode, 'w') || strchr(mode, 'a') || strchr(mode, '+'));

    if (is_read_denied(path)) {
        errno = EACCES;
        return NULL;
    }
    if (is_write_denied(path) && writing) {
        errno = EACCES;
        return NULL;
    }

    orig_fopen_t orig = (orig_fopen_t)dlsym(RTLD_NEXT, "fopen64");
    return orig(path, mode);
}
