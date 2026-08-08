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

def compare_images(expected_png, actual_png):
    try:
        from PIL import Image, ImageChops
        img1 = Image.open(expected_png)
        img2 = Image.open(actual_png)
        diff = ImageChops.difference(img1, img2)
        return not diff.getbbox()
    except Exception:
        # Fallback to byte comparison if Pillow is not available
        with open(expected_png, 'rb') as f1, open(actual_png, 'rb') as f2:
            return f1.read() == f2.read()

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
