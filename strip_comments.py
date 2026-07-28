#!/usr/bin/env python3
"""Remove comments from Rust source files without breaking strings or code."""
import sys
import os
import re

def remove_comments(filepath):
    with open(filepath, "r") as f:
        content = f.read()

    result = []
    i = 0
    n = len(content)

    while i < n:
        # String literal
        if content[i] == '"':
            result.append('"')
            i += 1
            while i < n:
                if content[i] == '\\':
                    result.append(content[i:i+2])
                    i += 2
                elif content[i] == '"':
                    result.append('"')
                    i += 1
                    break
                else:
                    result.append(content[i])
                    i += 1

        # Byte string literal b"..."
        elif content[i] == 'b' and i + 1 < n and content[i+1] == '"':
            result.append('b"')
            i += 2
            while i < n:
                if content[i] == '\\':
                    result.append(content[i:i+2])
                    i += 2
                elif content[i] == '"':
                    result.append('"')
                    i += 1
                    break
                else:
                    result.append(content[i])
                    i += 1

        # Char literal (but NOT lifetime like 'static or 'a)
        elif content[i] == '\'' and (i + 1 < n and not content[i+1].isalpha()):
            result.append("'")
            i += 1
            while i < n:
                if content[i] == '\\':
                    result.append(content[i:i+2])
                    i += 2
                elif content[i] == '\'':
                    result.append("'")
                    i += 1
                    break
                else:
                    result.append(content[i])
                    i += 1

        # Lifetime 'static, 'a, etc — skip the tick, keep the rest
        elif content[i] == '\'' and (i + 1 < n and content[i+1].isalpha()):
            result.append("'")
            i += 1

        # Block comment /* ... */ (Rust supports nesting)
        elif content[i] == '/' and i + 1 < n and content[i+1] == '*':
            depth = 1
            i += 2
            while i < n and depth > 0:
                if content[i] == '/' and i + 1 < n and content[i+1] == '*':
                    depth += 1
                    i += 2
                elif content[i] == '*' and i + 1 < n and content[i+1] == '/':
                    depth -= 1
                    i += 2
                else:
                    i += 1

        # Line comment // or doc comment /// or //!
        elif content[i] == '/' and i + 1 < n and content[i+1] == '/':
            # Skip to end of line
            while i < n and content[i] != '\n':
                i += 1

        else:
            result.append(content[i])
            i += 1

    cleaned = "".join(result)

    # Remove trailing whitespace on each line and collapse 3+ blank lines to 2
    lines = cleaned.split("\n")
    lines = [line.rstrip() for line in lines]

    # Collapse multiple blank lines
    final = []
    blank_count = 0
    for line in lines:
        if line.strip() == "":
            blank_count += 1
            if blank_count <= 2:
                final.append(line)
        else:
            blank_count = 0
            final.append(line)

    cleaned = "\n".join(final)
    # Ensure single trailing newline
    cleaned = cleaned.rstrip("\n") + "\n"

    with open(filepath, "w") as f:
        f.write(cleaned)

    return len(content) - len(cleaned)

def main():
    src_dir = os.path.join(os.path.dirname(os.path.abspath(__file__)), "src")
    total_saved = 0
    files_changed = 0

    for root, dirs, files in os.walk(src_dir):
        for fname in sorted(files):
            if fname.endswith(".rs"):
                filepath = os.path.join(root, fname)
                before = os.path.getsize(filepath)
                saved = remove_comments(filepath)
                after = os.path.getsize(filepath)
                if saved > 0:
                    files_changed += 1
                    total_saved += saved
                    print(f"  {os.path.relpath(filepath, src_dir)}: {before} -> {after} bytes (-{saved})")

    print(f"\nDone: {files_changed} files cleaned, {total_saved} bytes removed")

if __name__ == "__main__":
    main()
