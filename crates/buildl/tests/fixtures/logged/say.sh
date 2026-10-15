#!/bin/sh
printf "said %s\n" "$1"
printf "warned\n" >&2
while IFS= read -r line; do printf "%s\n" "$line"; done < "$1" > "$2"
