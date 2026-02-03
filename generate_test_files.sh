#!/bin/bash

# Generate test file structure for search testing
BASE="data"

mkdir -p "$BASE"

# Documents
mkdir -p "$BASE/docs/work/reports/2024"
mkdir -p "$BASE/docs/work/presentations"
mkdir -p "$BASE/docs/personal/taxes/2023"
mkdir -p "$BASE/docs/personal/recipes"

touch "$BASE/docs/work/reports/2024/quarterly_summary.pdf"
touch "$BASE/docs/work/reports/2024/annual_review.docx"
touch "$BASE/docs/work/reports/budget_forecast.xlsx"
touch "$BASE/docs/work/presentations/product_launch.pptx"
touch "$BASE/docs/work/presentations/team_meeting.pptx"
touch "$BASE/docs/work/meeting_notes.txt"
touch "$BASE/docs/personal/taxes/2023/w2_form.pdf"
touch "$BASE/docs/personal/taxes/2023/receipts.pdf"
touch "$BASE/docs/personal/recipes/lasagna.md"
touch "$BASE/docs/personal/recipes/chocolate_cake.md"
touch "$BASE/docs/personal/resume.docx"
touch "$BASE/docs/readme.txt"

# Movies
mkdir -p "$BASE/movies/action/marvel"
mkdir -p "$BASE/movies/comedy/classics"
mkdir -p "$BASE/movies/documentary/nature"
mkdir -p "$BASE/movies/scifi/2020s"

touch "$BASE/movies/action/marvel/ironman.mkv"
touch "$BASE/movies/action/marvel/avengers_endgame.mp4"
touch "$BASE/movies/action/die_hard.mkv"
touch "$BASE/movies/action/mad_max_fury_road.mp4"
touch "$BASE/movies/comedy/classics/airplane.avi"
touch "$BASE/movies/comedy/classics/ghostbusters.mkv"
touch "$BASE/movies/comedy/superbad.mp4"
touch "$BASE/movies/documentary/nature/planet_earth.mkv"
touch "$BASE/movies/documentary/nature/blue_planet.mp4"
touch "$BASE/movies/documentary/free_solo.mkv"
touch "$BASE/movies/scifi/2020s/dune.mkv"
touch "$BASE/movies/scifi/2020s/tenet.mp4"
touch "$BASE/movies/scifi/blade_runner_2049.mkv"

# Backups
mkdir -p "$BASE/backups/daily/2024-01"
mkdir -p "$BASE/backups/weekly/2024"
mkdir -p "$BASE/backups/databases/postgres"
mkdir -p "$BASE/backups/databases/mysql"

touch "$BASE/backups/daily/2024-01/backup_20240115.tar.gz"
touch "$BASE/backups/daily/2024-01/backup_20240116.tar.gz"
touch "$BASE/backups/daily/2024-01/backup_20240117.tar.gz"
touch "$BASE/backups/weekly/2024/week01.tar.gz"
touch "$BASE/backups/weekly/2024/week02.tar.gz"
touch "$BASE/backups/databases/postgres/users_dump.sql"
touch "$BASE/backups/databases/postgres/orders_dump.sql"
touch "$BASE/backups/databases/mysql/inventory.sql.gz"
touch "$BASE/backups/databases/mysql/logs.sql.gz"
touch "$BASE/backups/full_system_backup.iso"
touch "$BASE/backups/config_backup.zip"

echo "Created test file structure in $BASE/"
find "$BASE" -type f | wc -l | xargs echo "Total files:"
