#!/bin/bash

  API="http://34.72.35.96:8080/api/ingest/async"
  FOLDER="${1:-.}"

  echo "Scanning: $FOLDER"
  echo "API: $API"
  echo "---"

  count=0
  success=0
  failed=0

  find "$FOLDER" -type f \( \
      -iname "*.pdf" -o \
      -iname "*.docx" -o \
      -iname "*.doc" -o \
      -iname "*.pptx" -o \
      -iname "*.ppt" -o \
      -iname "*.xlsx" -o \
      -iname "*.xls" -o \
      -iname "*.txt" -o \
      -iname "*.md" -o \
      -iname "*.csv" -o \
      -iname "*.html" -o \
      -iname "*.rtf" \
  \) -print0 | while IFS= read -r -d '' file; do
      ((count++))
      filename=$(basename "$file")
      echo "[$count] Uploading: $filename"

      response=$(curl -s -X POST "$API" -F "files=@$file" 2>&1)

      if echo "$response" | grep -q '"success":true'; then
          echo "    ✓ Success"
          ((success++))
      else
          echo "    ✗ Failed"
          ((failed++))
      fi
  done

  echo ""
  echo "Done!"

