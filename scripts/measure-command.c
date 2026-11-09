/* Linux benchmark helper: measure the executable child without inheriting the
 * Python benchmark driver's resident-memory high-water mark before exec. */
#define _GNU_SOURCE
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/resource.h>
#include <sys/wait.h>
#include <unistd.h>

int main(int argc, char **argv) {
    if (argc < 3) {
        fprintf(stderr, "usage: measure-command METRICS.json EXECUTABLE [ARG...]\n");
        return 2;
    }
    pid_t child = fork();
    if (child == -1) { perror("fork"); return 1; }
    if (child == 0) {
        execv(argv[2], argv + 2);
        perror("execv");
        _exit(127);
    }
    int status;
    struct rusage usage;
    while (wait4(child, &status, 0, &usage) == -1) {
        if (errno != EINTR) { perror("wait4"); return 1; }
    }
    FILE *output = fopen(argv[1], "w");
    if (!output) { perror("metrics file"); return 1; }
    int written = fprintf(output,
        "{\"user_s\": %.6f, \"system_s\": %.6f, \"peak_rss_kib\": %ld}\n",
        usage.ru_utime.tv_sec + usage.ru_utime.tv_usec / 1000000.0,
        usage.ru_stime.tv_sec + usage.ru_stime.tv_usec / 1000000.0,
        usage.ru_maxrss);
    int closed = fclose(output);
    if (written < 0 || closed != 0) { perror("writing metrics"); return 1; }
    return WIFEXITED(status) ? WEXITSTATUS(status) : 128 + WTERMSIG(status);
}
