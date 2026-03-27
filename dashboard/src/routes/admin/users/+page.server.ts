import { error } from '@sveltejs/kit';
import type { PageServerLoad, Actions } from './$types';
import { query } from '$lib/db';

export const load: PageServerLoad = async (event) => {
  const session = await event.locals.auth();
  const userRole = (session?.user as any)?.role;
  
  if (userRole !== 'admin') throw error(403, 'Forbidden');

  const result = await query(
    'SELECT id, name, email, role, image FROM users ORDER BY role DESC, email ASC'
  );

  return {
    users: result.rows
  };
};

export const actions: Actions = {
  updateRole: async (event) => {
    const session = await event.locals.auth();
    if ((session?.user as any)?.role !== 'admin') throw error(403, 'Forbidden');

    const formData = await event.request.formData();
    const userId = formData.get('userId');
    const newRole = formData.get('role');

    if (!userId || !newRole) throw error(400, 'Missing parameters');
    
    // Prevent self-demotion or self-blocking
    if (userId === (session?.user as any)?.id) {
        return { success: false, message: "You cannot change your own role." };
    }

    await query('UPDATE users SET role = $1 WHERE id = $2', [newRole, userId]);
    return { success: true };
  },
  
  deleteUser: async (event) => {
    const session = await event.locals.auth();
    if ((session?.user as any)?.role !== 'admin') throw error(403, 'Forbidden');

    const formData = await event.request.formData();
    const userId = formData.get('userId');

    if (!userId) throw error(400, 'Missing user ID');
    
    if (userId === (session?.user as any)?.id) {
        return { success: false, message: "You cannot delete yourself." };
    }

    await query('DELETE FROM users WHERE id = $1', [userId]);
    return { success: true };
  }
};
