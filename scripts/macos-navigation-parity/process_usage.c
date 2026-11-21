/* One Darwin process sample; intentionally does not launch or signal the PID.
 *
 * Unit contract, Apple XNU f6217f891ac0bb64f3d375211650a4c1ff8ca1ea:
 * osfmk/kern/bsd_kern.c:1187-1211 fills rusage CPU and memory fields.
 * osfmk/kern/task.c:6807-6812 supplies CPU time in Mach timebase units;
 * osfmk/kern/recount.h:121-123 states that unit explicitly.
 * osfmk/kern/task.c:1283-1284,1299-1300 registers memory ledgers in bytes.
 * Convert CPU tick deltas to ns using timebase_numer / timebase_denom.
 * Instructions/cycles are raw counters; zero does not establish availability.
 */
#include <errno.h>
#include <inttypes.h>
#include <libproc.h>
#include <limits.h>
#include <mach/mach_time.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/resource.h>
#include <time.h>

static int fail(const char *operation, int error_code, int kernel_code) {
    printf("{\"schema\":\"macos-process-usage-v1\",\"error_operation\":\"%s\","
           "\"errno\":%d,\"kern_return\":%d}\n",
           operation, error_code, kernel_code);
    fprintf(stderr, "%s: %s (kern_return=%d)\n", operation,
            strerror(error_code), kernel_code);
    return 1;
}

static int monotonic_ns(uint64_t *value) {
    struct timespec now;
    if (clock_gettime(CLOCK_MONOTONIC, &now) != 0) {
        return -1;
    }
    *value = (uint64_t)now.tv_sec * UINT64_C(1000000000) +
             (uint64_t)now.tv_nsec;
    return 0;
}

int main(int argc, char **argv) {
    if (argc != 2 || argv[1][0] < '0' || argv[1][0] > '9') {
        return fail("pid_argument", EINVAL, 0);
    }
    char *end;
    errno = 0;
    long parsed_pid = strtol(argv[1], &end, 10);
    if (errno != 0 || *end != '\0' || parsed_pid <= 0 || parsed_pid > INT_MAX) {
        return fail("pid_argument", errno != 0 ? errno : EINVAL, 0);
    }

    mach_timebase_info_data_t timebase;
    kern_return_t status = mach_timebase_info(&timebase);
    if (status != KERN_SUCCESS || timebase.numer == 0 || timebase.denom == 0) {
        return fail("mach_timebase_info", EIO, status);
    }

    uint64_t begin, finish;
    if (monotonic_ns(&begin) != 0) {
        return fail("clock_gettime_begin", errno, 0);
    }
    struct rusage_info_v4 usage = {0};
    if (proc_pid_rusage((int)parsed_pid, RUSAGE_INFO_V4,
                        (rusage_info_t *)&usage) != 0) {
        return fail("proc_pid_rusage", errno, 0);
    }
    if (monotonic_ns(&finish) != 0) {
        return fail("clock_gettime_end", errno, 0);
    }
    /* Midpoint plus both bounds exposes the non-atomic sample's uncertainty. */
    uint64_t midpoint = begin + (finish - begin) / 2;
    printf("{\"schema\":\"macos-process-usage-v1\",\"pid\":%ld,"
           "\"cpu_time_unit\":\"mach_absolute_ticks\","
           "\"user_time_mach_ticks\":%" PRIu64 ","
           "\"system_time_mach_ticks\":%" PRIu64 ","
           "\"timebase_numer\":%" PRIu32 ",\"timebase_denom\":%" PRIu32 ","
           "\"resident_bytes\":%" PRIu64 ",\"phys_footprint_bytes\":%" PRIu64 ","
           "\"instructions\":%" PRIu64 ",\"cycles\":%" PRIu64 ","
           "\"proc_start_mach_ticks\":%" PRIu64 ","
           "\"sample_monotonic_ns\":%" PRIu64 ","
           "\"sample_begin_monotonic_ns\":%" PRIu64 ","
           "\"sample_end_monotonic_ns\":%" PRIu64 "}\n",
           parsed_pid, usage.ri_user_time, usage.ri_system_time,
           timebase.numer, timebase.denom,
           usage.ri_resident_size, usage.ri_phys_footprint,
           usage.ri_instructions, usage.ri_cycles, usage.ri_proc_start_abstime,
           midpoint, begin, finish);
    return ferror(stdout) ? 1 : 0;
}
