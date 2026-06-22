#!/usr/bin/env bash
set -euo pipefail

zvec_dir="${1:-_zvec}"
memory_helper="${zvec_dir}/src/ailego/utility/memory_helper.cc"
bitpacked_posting_list="${zvec_dir}/src/db/index/column/fts_column/posting/bitpacked_posting_list.cc"

if [[ ! -f "${memory_helper}" ]]; then
  echo "missing zvec memory helper: ${memory_helper}" >&2
  exit 1
fi
if [[ ! -f "${bitpacked_posting_list}" ]]; then
  echo "missing zvec bitpacked posting list: ${bitpacked_posting_list}" >&2
  exit 1
fi

# zvec v0.5.0 uses std::aligned_alloc, but Android API 21 does not provide the
# C symbol behind libc++'s using declaration. The release assets target API 21,
# so use the POSIX allocator on Android while keeping upstream behavior elsewhere.
if ! grep -q 'posix_memalign(&ptr, alignment, aligned_size)' "${memory_helper}"; then
  perl -0pi -e 's/#include <cstdio>\n#include <cstring>/#include <cstdio>\n#include <cstdlib>\n#include <cstring>/' "${memory_helper}"
  perl -0pi -e 's/#if defined\(_WIN64\) \|\| defined\(_WIN32\)\n  void \*ptr = ::_aligned_malloc\(aligned_size, alignment\);\n#else\n  void \*ptr = std::aligned_alloc\(alignment, aligned_size\);\n#endif/#if defined(_WIN64) || defined(_WIN32)\n  void *ptr = ::_aligned_malloc(aligned_size, alignment);\n#elif defined(__ANDROID__)\n  void *ptr = nullptr;\n  if (::posix_memalign(\&ptr, alignment, aligned_size) != 0) {\n    ptr = nullptr;\n  }\n#else\n  void *ptr = std::aligned_alloc(alignment, aligned_size);\n#endif/' "${memory_helper}"
fi

if ! grep -q 'posix_memalign(&raw_ptr, 16, num_bytes)' "${bitpacked_posting_list}"; then
  perl -0pi -e 's/#ifdef _MSC_VER\n  auto \*ptr = static_cast<uint32_t \*>\(_aligned_malloc\(num_bytes, 16\)\);\n  return std::unique_ptr<uint32_t\[\], decltype\(&_aligned_free\)>\(ptr,\n                                                               _aligned_free\);\n#else\n  auto \*ptr = static_cast<uint32_t \*>\(std::aligned_alloc\(16, num_bytes\)\);\n  return std::unique_ptr<uint32_t\[\], decltype\(&std::free\)>\(ptr, std::free\);\n#endif/#ifdef _MSC_VER\n  auto *ptr = static_cast<uint32_t *>(_aligned_malloc(num_bytes, 16));\n  return std::unique_ptr<uint32_t[], decltype(&_aligned_free)>(ptr,\n                                                               _aligned_free);\n#elif defined(__ANDROID__)\n  void *raw_ptr = nullptr;\n  if (::posix_memalign(\&raw_ptr, 16, num_bytes) != 0) {\n    raw_ptr = nullptr;\n  }\n  auto *ptr = static_cast<uint32_t *>(raw_ptr);\n  return std::unique_ptr<uint32_t[], decltype(&std::free)>(ptr, std::free);\n#else\n  auto *ptr = static_cast<uint32_t *>(std::aligned_alloc(16, num_bytes));\n  return std::unique_ptr<uint32_t[], decltype(&std::free)>(ptr, std::free);\n#endif/' "${bitpacked_posting_list}"
fi

grep -q 'posix_memalign(&ptr, alignment, aligned_size)' "${memory_helper}"
grep -q 'posix_memalign(&raw_ptr, 16, num_bytes)' "${bitpacked_posting_list}"
