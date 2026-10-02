// Isolated lifecycle instrumentation: suppress every numeric SIGKILL request.
// No PID reuse or unrelated-process signal is needed to prove lost authority.
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <spawn.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/wait.h>

static void record(const char *kind, int pid, int result) {
    const char *path = getenv("DAEMON_CHILD_TRACE");
    if (!path) return;
    int saved = errno;
    int fd = open(path, O_WRONLY | O_CREAT | O_APPEND | O_CLOEXEC, 0600);
    if (fd >= 0) {
        char row[128];
        int len = snprintf(row, sizeof row, "%s %d %d\n", kind, pid, result);
        if (len > 0 && len < (int)sizeof row) (void)write(fd, row, len);
        close(fd);
    }
    errno = saved;
}

static void auto_reap(void) {
    struct sigaction action = {0};
    action.sa_handler = SIG_DFL;
    action.sa_flags = SA_NOCLDWAIT;
    sigemptyset(&action.sa_mask);
    if (sigaction(SIGCHLD, &action, NULL)) _exit(120);
}

__attribute__((constructor)) static void setup(void) {
    if (getenv("DAEMON_INITIAL_NOCLDWAIT")) auto_reap();
}

int posix_spawn(pid_t *pid, const char *path, const posix_spawn_file_actions_t *files,
                const posix_spawnattr_t *attr, char *const argv[], char *const envp[]) {
    int (*real_spawn)(pid_t *, const char *, const posix_spawn_file_actions_t *,
                      const posix_spawnattr_t *, char *const[], char *const[]) =
        dlsym(RTLD_NEXT, "posix_spawn");
    // Fault injection violates the retention contract AFTER the editor's
    // pre-spawn check; cleanup must deny signalling even before try_wait.
    if (getenv("DAEMON_LOSE_CHILD_RETENTION")) { auto_reap(); record("LOSS", getpid(), 0); }
    return real_spawn(pid, path, files, attr, argv, envp);
}

int posix_spawnp(pid_t *pid, const char *path, const posix_spawn_file_actions_t *files,
                 const posix_spawnattr_t *attr, char *const argv[], char *const envp[]) {
    int (*real_spawn)(pid_t *, const char *, const posix_spawn_file_actions_t *,
                      const posix_spawnattr_t *, char *const[], char *const[]) =
        dlsym(RTLD_NEXT, "posix_spawnp");
    if (getenv("DAEMON_LOSE_CHILD_RETENTION")) { auto_reap(); record("LOSS", getpid(), 0); }
    return real_spawn(pid, path, files, attr, argv, envp);
}

pid_t waitpid(pid_t pid, int *status, int opts) {
    pid_t (*real_wait)(pid_t, int *, int) = dlsym(RTLD_NEXT, "waitpid");
    pid_t result = real_wait(pid, status, opts);
    if (result < 0 && errno == ECHILD) record("ECHILD", pid, 0);
    return result;
}

int kill(pid_t pid, int sig) {
    int (*real_kill)(pid_t, int) = dlsym(RTLD_NEXT, "kill");
    if (sig == SIGKILL && getenv("DAEMON_CHILD_TRACE")) {
        record("SUPPRESSED_KILL", pid, sig);
        errno = ESRCH;
        return -1;
    }
    return real_kill(pid, sig);
}
