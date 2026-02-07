/**
 * API route definitions and configuration.
 */

import { AuthMiddleware } from '../auth/middleware';
import { handleLogin, handleRegister, handleGetUser, Request, Response } from './handlers';
import { AuthService } from '../auth/service';
import { UserRepository } from '../db/user-repo';
import { Logger } from '../utils/logger';

const logger = new Logger('Routes');

export interface Route {
    method: string;
    path: string;
    handler: (req: Request) => Promise<Response>;
    requiresAuth: boolean;
}

/**
 * Configure and return all API routes.
 */
export function configureRoutes(
    authMiddleware: AuthMiddleware,
    authService: AuthService,
    userRepo: UserRepository
): Route[] {
    logger.info('Configuring API routes');

    const routes: Route[] = [
        {
            method: 'POST',
            path: '/api/login',
            handler: (req) => handleLogin(req, authService),
            requiresAuth: false,
        },
        {
            method: 'POST',
            path: '/api/register',
            handler: (req) => handleRegister(req, userRepo),
            requiresAuth: false,
        },
        {
            method: 'GET',
            path: '/api/users/:id',
            handler: (req) => handleGetUser(req, userRepo),
            requiresAuth: true,
        },
    ];

    return routes;
}
