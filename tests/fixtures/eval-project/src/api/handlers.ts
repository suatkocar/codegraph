/**
 * Request handlers for API endpoints.
 */

import { AuthService } from '../auth/service';
import { UserRepository } from '../db/user-repo';
import { Logger } from '../utils/logger';

const logger = new Logger('Handlers');

export interface Request {
    body: Record<string, unknown>;
    params: Record<string, string>;
    headers: Record<string, string>;
}

export interface Response {
    status: number;
    body: unknown;
}

/**
 * Handle user login requests.
 */
export async function handleLogin(
    req: Request,
    authService: AuthService
): Promise<Response> {
    const { email, password } = req.body as { email: string; password: string };
    logger.info(`Handling login for ${email}`);

    const result = await authService.login(email, password);
    if (result.success) {
        return { status: 200, body: { token: result.session?.token } };
    }
    return { status: 401, body: { error: result.error } };
}

/**
 * Handle user registration.
 */
export async function handleRegister(
    req: Request,
    userRepo: UserRepository
): Promise<Response> {
    const { email, name, password } = req.body as {
        email: string;
        name: string;
        password: string;
    };
    logger.info(`Handling registration for ${email}`);

    const user = await userRepo.create({ email, name, passwordHash: password });
    return { status: 201, body: { user } };
}

/**
 * Handle get user by ID.
 */
export async function handleGetUser(
    req: Request,
    userRepo: UserRepository
): Promise<Response> {
    const { id } = req.params;
    logger.info(`Handling get user: ${id}`);

    const user = await userRepo.findById(id);
    if (!user) {
        return { status: 404, body: { error: 'User not found' } };
    }
    return { status: 200, body: { user } };
}
