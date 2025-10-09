import re
import sys
from pathlib import Path

# --- Regex patterns ---
BOOTLOADER_MEMORY_LINE = re.compile(
    r'MemoryMapEntry { coverage: Range\(PhysicalAddress { addr: (\w+) }, PhysicalAddress { addr: (\w+) }\), ty: (\w+) }'
)
ALLOC_LINE = re.compile(
    r'(.*):(\d+) - === (\w{3}) (a|d) (0x[0-9a-f]+) -> (0x[0-9a-f]+)(?: ?: ?(.*))?',
    re.IGNORECASE
)

ALLOWED_TYPES = {'Free', 'BootloaderCode'}

def parse_bootloader_map_full(log_text):
    regions = []
    for match in BOOTLOADER_MEMORY_LINE.finditer(log_text):
        start = int(match.group(1), 16)
        end = int(match.group(2), 16)
        kind = match.group(3)
        regions.append((start, end, kind))
    return regions

def get_allowed_ranges(regions):
    return [(start, end) for start, end, kind in regions if kind in ALLOWED_TYPES]

def in_allowed_region(addr, allowed_ranges):
    return any(start <= addr < end for start, end in allowed_ranges)

def find_region_type(addr, regions):
    for start, end, kind in regions:
        if start <= addr < end:
            return kind
    return "Unknown"

def parse_kernel_allocations(log_lines, allowed_ranges, all_regions):
    allocated_ranges = dict()  # maps (start, end) -> (allocator, file, lineno, note)
    errors = []

    def recompute_total():
        return sum(end - start for (start, end) in allocated_ranges)

    def add_alloc(start, end, meta):
        allocated_ranges[(start, end)] = meta

    def remove_alloc(start, end, current_meta):
        # Find containing range
        for (a_start, a_end), meta in list(allocated_ranges.items()):
            if a_start <= start and end <= a_end:
                # Exact match
                if start == a_start and end == a_end:
                    del allocated_ranges[(a_start, a_end)]
                # Trim left
                elif start == a_start:
                    del allocated_ranges[(a_start, a_end)]
                    add_alloc(end, a_end, meta)
                # Trim right
                elif end == a_end:
                    del allocated_ranges[(a_start, a_end)]
                    add_alloc(a_start, start, meta)
                # Split into two
                else:
                    del allocated_ranges[(a_start, a_end)]
                    add_alloc(a_start, start, meta)
                    add_alloc(end, a_end, meta)
                return
        # If no containing range was found
        errors.append({
            "type": "invalid_dealloc",
            "range": (start, end),
            "current": current_meta
        })

    for line in log_lines:
        match = ALLOC_LINE.search(line)
        if not match:
            continue

        location, line_no, allocator, action, start_hex, end_hex, note = match.groups()
        start = int(start_hex, 16)
        end = int(end_hex, 16)
        meta = (allocator, location, line_no, note or "")

        if action == 'a':
            # Out of bounds check
            if not in_allowed_region(start, allowed_ranges) or not in_allowed_region(end - 1, allowed_ranges):
                region_type_start = find_region_type(start, all_regions)
                region_type_end = find_region_type(end - 1, all_regions)
                errors.append({
                    "type": "out_of_bounds",
                    "range": (start, end),
                    "current": meta,
                    "region_start": region_type_start,
                    "region_end": region_type_end
                })

            else:
                # Overlap check
                overlap = [(a_start, a_end, meta) for (a_start, a_end), meta in allocated_ranges.items()
                           if not (end <= a_start or start >= a_end)]
                if overlap:
                    prev_range, prev_meta = (overlap[0][0:2], overlap[0][2])
                    errors.append({
                        "type": "duplicate",
                        "range": (start, end),
                        "current": meta,
                        "previous": (prev_meta, prev_range[0], prev_range[1]),
                    })
                else:
                    add_alloc(start, end, meta)

        elif action == 'd':
            remove_alloc(start, end, meta)

    total_allocated = recompute_total()
    return errors, total_allocated


def format_bytes(size):
    units = ['B', 'KiB', 'MiB', 'GiB']
    i = 0
    while size >= 1024 and i < len(units) - 1:
        size /= 1024.0
        i += 1
    return f"{size:.2f} {units[i]}"

def main():
    if len(sys.argv) < 2:
        print("Usage: python check_bitmap_allocations.py <kernel_log_file>")
        sys.exit(1)

    log_path = Path(sys.argv[1])
    if not log_path.exists():
        print(f"Error: file not found: {log_path}")
        sys.exit(1)

    with open(log_path, "r") as f:
        log_text = f.read()
        log_lines = log_text.splitlines()

    all_regions = parse_bootloader_map_full(log_text)
    allowed_ranges = get_allowed_ranges(all_regions)

    print(f"🧠 Parsed {len(all_regions)} memory regions, {len(allowed_ranges)} allowed for allocations.")

    errors, total_allocated = parse_kernel_allocations(log_lines, allowed_ranges, all_regions)

    if not errors:
        print("\n✅ No duplicate or invalid allocations detected.")
    else:
        print(f"\n❌ Detected {len(errors)} errors:\n")
        for e in errors:
            r = e["range"]
            start, end = r
            if e["type"] == "duplicate":
                cur = e["current"]
                prev = e["previous"]
                print(f"[DUPLICATE] {hex(start)} -> {hex(end)}")
                print(f"   Current : {cur[0]} at {cur[1]}:{cur[2]}")
                print(f"   Previous: {prev[0][0]} at {prev[0][1]}:{prev[0][2]} ({hex(prev[1])} -> {hex(prev[2])})")
                note_str = f"   Note     : {cur[3]}" if cur[3] else ""
                if note_str:
                    print(note_str)
            elif e["type"] == "out_of_bounds":
                cur = e["current"]
                rs = e["region_start"]
                re = e["region_end"]
                print(f"[INVALID ALLOCATION] {hex(start)} -> {hex(end)} (not in allowed regions)")
                print(f"   Allocator: {cur[0]} at {cur[1]}:{cur[2]}")
                print(f"   Region types: start={rs}, end={re}")
                note_str = f"   Note     : {cur[3]}" if cur[3] else ""
                if note_str:
                    print(note_str)
            elif e["type"] == "invalid_dealloc":
                cur = e["current"]
                print(f"[INVALID DEALLOC] {hex(start)} -> {hex(end)} was never allocated")
                print(f"   Deallocator: {cur[0]} at {cur[1]}:{cur[2]}")

    print(f"\n📊 Total memory currently allocated: {format_bytes(total_allocated)}")

if __name__ == "__main__":
    main()
