import sqlite3
import psycopg2
from psycopg2.extras import execute_values
import os
import sys

# Configuration - Update these to match your .env or pass as arguments
SQLITE_DB = os.getenv("SQLITE_DB", "data/nmcscan.db")
POSTGRES_URL = os.getenv("DATABASE_URL", "postgres://nmcscan:nmcscan_secret@localhost:5432/nmcscan")

def migrate():
    if not os.path.exists(SQLITE_DB):
        print(f"❌ SQLite database not found at {SQLITE_DB}")
        sys.exit(1)

    print(f"🚀 Starting migration from {SQLITE_DB} to PostgreSQL...")
    
    try:
        sl_conn = sqlite3.connect(SQLITE_DB)
        pg_conn = psycopg2.connect(POSTGRES_URL)
        pg_cur = pg_conn.cursor()

        tables = [
            "asns",
            "asn_ranges",
            "servers",
            "server_players",
            "server_history",
            "daily_stats"
        ]

        for table in tables:
            # Check if table exists in SQLite
            sl_cur = sl_conn.cursor()
            sl_cur.execute(f"SELECT name FROM sqlite_master WHERE type='table' AND name='{table}'")
            if not sl_cur.fetchone():
                print(f"⚠️ Table {table} does not exist in SQLite, skipping.")
                continue

            print(f"📦 Migrating table: {table}...")
            
            # Get data from SQLite
            sl_cur.execute(f"SELECT * FROM {table}")
            rows = sl_cur.fetchall()
            
            if not rows:
                print(f"  - Table {table} is empty, skipping.")
                continue

            # Get column names
            columns = [description[0] for description in sl_cur.description]
            col_string = ",".join(columns)
            
            # Prepare conflict target for Postgres
            conflict_target = None
            if table == "servers":
                conflict_target = "(ip, port)"
            elif table == "asns":
                conflict_target = "(asn)"
            elif table == "asn_ranges":
                conflict_target = "(cidr)"
            elif table == "server_players":
                conflict_target = "(ip, port, player_name)"
            elif table == "daily_stats":
                conflict_target = "(date)"

            # Build insert query
            insert_query = f"INSERT INTO {table} ({col_string}) VALUES %s"
            if conflict_target:
                insert_query += f" ON CONFLICT {conflict_target} DO NOTHING"

            # Execute batch insert
            execute_values(pg_cur, insert_query, rows)
            print(f"  - Successfully migrated {len(rows)} rows.")

        pg_conn.commit()
        print("✅ Migration completed successfully!")

    except Exception as e:
        print(f"❌ Migration failed: {e}")
        if 'pg_conn' in locals():
            pg_conn.rollback()
    finally:
        if 'sl_conn' in locals():
            sl_conn.close()
        if 'pg_conn' in locals():
            pg_conn.close()

if __name__ == "__main__":
    migrate()
