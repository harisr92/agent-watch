#!/usr/bin/env python3
import sys
import time

def main():
    print("\033[1;34m[MockAgent]\033[0m Initializing agent session...")
    time.sleep(1)
    print("\033[1;34m[MockAgent]\033[0m Analyzing project repository...")
    time.sleep(1)
    print("\033[1;33m[MockAgent]\033[0m Found 14 outdated cache files.")
    time.sleep(0.5)
    
    # Prompt user
    prompt = "Do you want to run bash command: rm -rf .cache/build? [y/N]: "
    sys.stdout.write(prompt)
    sys.stdout.flush()

    response = sys.stdin.readline().strip()
    print(f"\n\033[1;32m[MockAgent]\033[0m Received user input: '{response}'")

    if response.lower() == 'y':
        print("\033[1;32m[MockAgent]\033[0m Execution approved! Removing cache...")
        time.sleep(1)
        print("\033[1;32m[MockAgent]\033[0m Done. Task completed successfully.")
        sys.exit(0)
    else:
        print("\033[1;31m[MockAgent]\033[0m Operation rejected by user. Aborting.")
        sys.exit(1)

if __name__ == "__main__":
    main()
