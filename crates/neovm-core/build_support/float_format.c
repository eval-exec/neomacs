/* GNU editfns.c:3890-3946: integers fitting intmax_t or uintmax_t are
   formatted as long double only if converting to double loses information.
   Rust has no ABI-compatible long double, so keep that conversion here. */
#include <float.h>
#include <inttypes.h>
#include <stdio.h>
#include <string.h>

int neovm_float_useful_precision(void)
{
  return (1 - LDBL_MIN_EXP) * (FLT_RADIX == 2 || FLT_RADIX == 10 ? 1
                              : FLT_RADIX == 16 ? 4 : -1);
}

static int format_long_double(char *buffer, size_t capacity,
                              const char *format, int precision,
                              long double value)
{
  char long_format[16];
  size_t length = strlen(format);
  /* Rust passes only % followed by +/space/#, .*, and e/f/g, so the
     format is at most eight bytes, excluding its terminating null. */
  memcpy(long_format, format, length - 1);
  long_format[length - 1] = 'L';
  long_format[length] = format[length - 1];
  long_format[length + 1] = 0;
  return snprintf(buffer, capacity, long_format, precision, value);
}

int neovm_float_format_signed(char *buffer, size_t capacity,
                             const char *format, int precision, int64_t value)
{
  double number = value;
  if (LDBL_MANT_DIG > DBL_MANT_DIG && number != (long double) value)
    return format_long_double(buffer, capacity, format, precision, value);
  return snprintf(buffer, capacity, format, precision, number);
}

int neovm_float_format_unsigned(char *buffer, size_t capacity,
                               const char *format, int precision, uint64_t value)
{
  double number = value;
  if (LDBL_MANT_DIG > DBL_MANT_DIG && number != (long double) value)
    return format_long_double(buffer, capacity, format, precision, value);
  return snprintf(buffer, capacity, format, precision, number);
}
