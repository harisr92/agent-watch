#!/usr/bin/env python3
import sys
import time

def main():
    print("\033[1;36m[ResearchAgent]\033[0m Starting research task...")
    time.sleep(1)
    prompt = "What should I search for next? > "
    sys.stdout.write(prompt)
    sys.stdout.flush()

    response = sys.stdin.readline().strip()
    print(f"\n\033[1;32m[ResearchAgent]\033[0m Researching query: '{response}'...")
    time.sleep(1)
    print("\033[1;32m[ResearchAgent]\033[0m Found 5 relevant articles. Analysis complete.")
    sys.exit(0)

if __name__ == "__main__":
    main()
