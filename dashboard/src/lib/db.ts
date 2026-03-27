import pg from 'pg';
import { env } from '$env/dynamic/private';

// Lazy initialize the pool
let pool: pg.Pool | null = null;

export function getPool() {
  if (!pool) {
    if (!env.DATABASE_URL) {
      console.warn('⚠️ DATABASE_URL is not set!');
    }
    pool = new pg.Pool({
      connectionString: env.DATABASE_URL
    });
    
    pool.on('error', (err) => {
      console.error('Unexpected error on idle client', err);
    });
  }
  return pool;
}

export async function query(text: string, params?: any[]) {
  return getPool().query(text, params);
}
