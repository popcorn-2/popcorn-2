import re
import sys
import argparse
from collections import defaultdict

PAGE_MAP_OFFSET = 0xffff_8000_0000_0000
PAGE_MAP_LEN = 1 << 46
PAGE_MAP_END = PAGE_MAP_OFFSET + PAGE_MAP_LEN

def in_page_map_region(va):
    return PAGE_MAP_OFFSET <= va < PAGE_MAP_END

def parse_allocations(log_lines):
    alloc_regex = re.compile(r"=== (\w{3}) [ad] (0x[0-9a-f]+) -> (0x[0-9a-f]+)(?: : (.+))?")
    allocations = {}

    for line in log_lines:
        match = alloc_regex.search(line)
        if not match:
            continue
        allocator, start_hex, end_hex, note = match.groups()
        start = int(start_hex, 16)
        end = int(end_hex, 16)
        note = note or ""
        for pa in range(start, end, 0x1000):  # 4KiB pages
            allocations[pa] = (allocator, note)

    return allocations

def parse_mappings(log_lines):
    map_regex = re.compile(r"(.*):(\d+) - === map va (0x[0-9a-f]+) -> pa (0x[0-9a-f]+)(?: : ty=(.+))?")
    unmap_regex = re.compile(r"(.*):(\d+) - === unmap va (0x[0-9a-f]+)")

    va_to_pa = {}
    pa_to_vas = defaultdict(set)
    va_info = {}

    for line in log_lines:
        map_match = map_regex.search(line)
        if map_match:
            file_path, line_num, va_hex, pa_hex, code_str = map_match.groups()
            va = int(va_hex, 16)
            pa = int(pa_hex, 16)
            code = str(code_str) if code_str else None
            va_to_pa[va] = pa
            pa_to_vas[pa].add(va)
            va_info[va] = {
                "code": code,
                "file": file_path.strip(),
                "line": int(line_num)
            }
            continue

        unmap_match = unmap_regex.search(line)
        if unmap_match:
            file_path, line_num, va_hex = unmap_match.groups()
            va = int(va_hex, 16)
            pa = va_to_pa.pop(va, None)
            va_info.pop(va, None)
            if pa is not None:
                pa_to_vas[pa].discard(va)
                if not pa_to_vas[pa]:
                    del pa_to_vas[pa]

    return pa_to_vas, va_info

def report_duplicates(pa_to_vas, va_info, allocations, include_page_map):
    print(f"=== Duplicate VA→PA Mappings ({'Including' if include_page_map else 'Excluding'} Page Map Region) ===\n")

    duplicates = 0

    for pa, va_set in sorted(pa_to_vas.items()):
        filtered_vas = (
            va_set if include_page_map else [va for va in va_set if not in_page_map_region(va)]
        )

        if len(filtered_vas) > 1:
            duplicates += 1
            allocator, note = allocations.get(pa, ("???", "no record"))
            print(f"PA: 0x{pa:016x} mapped {len(filtered_vas)} times — Allocator: {allocator}, Note: {note}")
            for va in sorted(filtered_vas):
                info = va_info.get(va, {})
                code = info.get("code", "None")
                file_path = info.get("file", "unknown")
                line_num = info.get("line", "?")
                print(f"  - VA: 0x{va:016x} [{code}] @ {file_path}:{line_num}")
            print()

    if duplicates == 0:
        print("No duplicate mappings found.")
    else:
        print(f"Found {duplicates} duplicate-mapped physical pages.")

def main():
    parser = argparse.ArgumentParser(description="Analyze VA→PA mappings and flag duplicate mappings.")
    parser.add_argument("log_file", help="Path to kernel log file")
    parser.add_argument("--include-page-map", action="store_true", help="Include the page map region in duplicate analysis")
    args = parser.parse_args()

    try:
        with open(args.log_file, "r") as f:
            lines = f.readlines()
    except Exception as e:
        print(f"Error reading file: {e}")
        sys.exit(1)

    allocations = parse_allocations(lines)
    pa_to_vas, va_info = parse_mappings(lines)
    report_duplicates(pa_to_vas, va_info, allocations, args.include_page_map)

if __name__ == "__main__":
    main()
