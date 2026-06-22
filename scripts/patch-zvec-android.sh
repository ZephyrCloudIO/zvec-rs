#!/usr/bin/env bash
set -euo pipefail

zvec_dir="${1:-_zvec}"
memory_helper="${zvec_dir}/src/ailego/utility/memory_helper.cc"

if [[ ! -f "${memory_helper}" ]]; then
  echo "missing zvec memory helper: ${memory_helper}" >&2
  exit 1
fi

if grep -q 'posix_memalign(&ptr, alignment, aligned_size)' "${memory_helper}"; then
  exit 0
fi

# zvec v0.5.0 uses std::aligned_alloc, but Android API 21 does not provide the
# C symbol behind libc++'s using declaration. The release assets target API 21,
# so use the POSIX allocator on Android while keeping upstream behavior elsewhere.
perl -0pi -e 's/#include <cstdio>\n#include <cstring>/#include <cstdio>\n#include <cstdlib>\n#include <cstring>/' "${memory_helper}"
perl -0pi -e 's/#if defined\(_WIN64\) \|\| defined\(_WIN32\)\n  void \*ptr = ::_aligned_malloc\(aligned_size, alignment\);\n#else\n  void \*ptr = std::aligned_alloc\(alignment, aligned_size\);\n#endif/#if defined(_WIN64) || defined(_WIN32)\n  void *ptr = ::_aligned_malloc(aligned_size, alignment);\n#elif defined(__ANDROID__)\n  void *ptr = nullptr;\n  if (::posix_memalign(\&ptr, alignment, aligned_size) != 0) {\n    ptr = nullptr;\n  }\n#else\n  void *ptr = std::aligned_alloc(alignment, aligned_size);\n#endif/' "${memory_helper}"

grep -q 'posix_memalign(&ptr, alignment, aligned_size)' "${memory_helper}"
