#!/usr/bin/env python3
import os
import sys
import subprocess
import argparse
import shutil

# Directories
SAMPLES_DIR = "samples"
REF_DIR = os.path.join(SAMPLES_DIR, "references")
ACTUAL_DIR = os.path.join("out", "visual_actual")
DIFF_DIR = os.path.join("out", "visual_diff")

# Sample PDF files and pages to verify
TEST_CASES = [
    {"pdf": "volvo_xc90.pdf", "pages": [1]},
    {"pdf": "constitution.pdf", "pages": [1]},
    {"pdf": "bokutokitan.pdf", "pages": [1]},
    {"pdf": "print_sample.pdf", "pages": [1]},
]

def ensure_binaries():
    print("Building fepdf binary...")
    try:
        subprocess.run(["cargo", "build", "--bin", "fepdf"], check=True)
        return "./target/debug/fepdf"
    except subprocess.CalledProcessError as e:
        print(f"Error: Failed to build fepdf binary: {e}")
        sys.exit(1)

def run_render(fepdf_bin, pdf_path, page, output_png):
    os.makedirs(os.path.dirname(output_png), exist_ok=True)
    cmd = [fepdf_bin, "publish", "render", pdf_path, output_png, "--page", str(page)]
    res = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    return res.returncode == 0, res.stdout, res.stderr

def decode_png_scanlines(path):
    import zlib, struct
    with open(path, "rb") as f:
        data = f.read()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        return None, None
    pos = 8
    size = None
    idat = []
    while pos < len(data):
        length = struct.unpack(">I", data[pos:pos+4])[0]
        chunk_type = data[pos+4:pos+8]
        chunk_data = data[pos+8:pos+8+length]
        if chunk_type == b"IHDR":
            size = struct.unpack(">II", chunk_data[:8])
        elif chunk_type == b"IDAT":
            idat.append(chunk_data)
        elif chunk_type == b"IEND":
            break
        pos += 8 + length + 4
    try:
        decompressed = zlib.decompress(b"".join(idat))
        return size, decompressed
    except Exception:
        return None, None


def compare_images(expected_png, actual_png):
    try:
        from PIL import Image, ImageChops
        img1 = Image.open(expected_png)
        img2 = Image.open(actual_png)
        if img1.size != img2.size:
            return False
        diff = ImageChops.difference(img1, img2)
        # Allow tiny subpixel rasterisation delta (max channel delta <= 1)
        stat = diff.getextrema()
        max_delta = max(s[1] for s in stat) if isinstance(stat[0], tuple) else stat[1]
        return max_delta <= 1
    except Exception:
        s1, p1 = decode_png_scanlines(expected_png)
        s2, p2 = decode_png_scanlines(actual_png)
        if s1 is None or s2 is None or s1 != s2 or len(p1) != len(p2):
            with open(expected_png, 'rb') as f1, open(actual_png, 'rb') as f2:
                return f1.read() == f2.read()
        diff_count = sum(1 for a, b in zip(p1, p2) if abs(a - b) > 1)
        # Pass if 99.99% of bytes match within delta <= 1
        return diff_count <= len(p1) * 0.0001

def main():
    parser = argparse.ArgumentParser(description="Ferruginous Visual Regression Test Suite")
    parser.add_argument("--update", action="store_true", help="Update the reference images with current rendering")
    args = parser.parse_args()

    fepdf_bin = ensure_binaries()

    if os.path.exists(ACTUAL_DIR):
        shutil.rmtree(ACTUAL_DIR)
    os.makedirs(ACTUAL_DIR, exist_ok=True)
    
    if os.path.exists(DIFF_DIR):
        shutil.rmtree(DIFF_DIR)
    os.makedirs(DIFF_DIR, exist_ok=True)

    if args.update:
        os.makedirs(REF_DIR, exist_ok=True)
        print("\n=== Updating Reference Baselines ===")
    else:
        print("\n=== Visual Regression Verification Starting ===")

    total = 0
    passed = 0
    failed = 0

    for case in TEST_CASES:
        pdf_name = case["pdf"]
        pdf_path = os.path.join(SAMPLES_DIR, pdf_name)
        
        if not os.path.exists(pdf_path):
            print(f"Warning: Sample file {pdf_path} not found. Skipping.")
            continue

        for page in case["pages"]:
            total += 1
            case_id = f"{pdf_name} (Page {page})"
            print(f"\nProcessing: {case_id}...")
            
            actual_png = os.path.join(ACTUAL_DIR, f"{pdf_name}_page_{page}.png")
            success, stdout, stderr = run_render(fepdf_bin, pdf_path, page, actual_png)
            
            if not success:
                print(f"  [RENDER FAIL] Failed to render page: {stderr.strip()}")
                failed += 1
                continue

            ref_png = os.path.join(REF_DIR, f"{pdf_name}_page_{page}.png")

            if args.update:
                shutil.copyfile(actual_png, ref_png)
                print(f"  [UPDATED] Reference baseline saved to {ref_png}")
                passed += 1
            else:
                if not os.path.exists(ref_png):
                    print(f"  [FAIL] Reference baseline missing: {ref_png}")
                    print("  Please run with --update to generate initial baseline references.")
                    failed += 1
                    continue
                
                match = compare_images(ref_png, actual_png)
                if match:
                    print("  [PASS] Render matches reference baseline.")
                    passed += 1
                else:
                    print("  [FAIL] Visual mismatch detected!")
                    failed += 1

    print("\n==========================================")
    print(f"Visual Test Results: {passed} PASSED, {failed} FAILED (Total: {total})")
    print("==========================================")

    if failed > 0:
        sys.exit(1)
    else:
        sys.exit(0)

if __name__ == "__main__":
    main()
