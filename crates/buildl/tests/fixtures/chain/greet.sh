#!/bin/sh
printf "words:\n" > "$2"
while IFS= read -r line; do printf "%s\n" "$line"; done < "$1" >> "$2"
