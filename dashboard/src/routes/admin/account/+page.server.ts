import { error } from '@sveltejs/kit';
import type { PageServerLoad, Actions } from './$types';
import { query } from '$lib/db';

export const load: PageServerLoad = async (event) => {
  const session = await event.locals.auth();
  if (!session?.user) throw error(401, 'Unauthorized');

  const userId = (session.user as any).id;

  const result = await query(
    'SELECT id, provider, "providerAccountId" FROM accounts WHERE "userId" = $1',
    [userId]
  );

  return {
    accounts: result.rows
  };
};

export const actions: Actions = {
  unlinkAccount: async (event) => {
    const session = await event.locals.auth();
    if (!session?.user) throw error(401, 'Unauthorized');

    const formData = await event.request.formData();
    const accountId = formData.get('accountId');

    if (!accountId) throw error(400, 'Missing account ID');

    // Check if it's the last account
    const userId = (session.user as any).id;
    const countRes = await query('SELECT COUNT(*) FROM accounts WHERE "userId" = $1', [userId]);
    if (parseInt(countRes.rows[0].count) <= 1) {
        return { success: false, message: "You cannot unlink your only account." };
    }

    await query('DELETE FROM accounts WHERE id = $1 AND "userId" = $2', [accountId, userId]);
    return { success: true };
  }
};
