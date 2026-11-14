#!/usr/bin/env python3
"""Mock agent that asks a numbered menu question, then a secret, then waits for Enter."""
import getpass
import sys
import time


def ask(prompt: str) -> str:
    sys.stdout.write(prompt)
    sys.stdout.flush()
    return sys.stdin.readline().strip()


def main():
    print("\033[1;35m[MenuAgent]\033[0m Planning database migration...")
    time.sleep(1)
    print("How should I apply the migration?")
    print("  1) Apply now")
    print("  2) Dry run first")
    print("  3) Skip this step")
    choice = ask("Enter your choice [1-3]: ")
    print(f"\033[1;32m[MenuAgent]\033[0m You picked option {choice!r}")

    time.sleep(0.5)
    token = getpass.getpass("Enter your API key: ")
    print(f"\033[1;32m[MenuAgent]\033[0m Got a key of {len(token)} characters")

    time.sleep(0.5)
    ask("Migration plan ready. Press Enter to continue...")
    print("\033[1;32m[MenuAgent]\033[0m Done.")


if __name__ == "__main__":
    main()
