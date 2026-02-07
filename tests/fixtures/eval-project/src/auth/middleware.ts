/**
 * Authentication middleware for Express routes.
 */

import { AuthService } from './service';
import { User, Session } from './types';
import { Logger } from '../utils/logger';

const logger = new Logger('AuthMiddleware');

export interface AuthenticatedRequest {
    user: User;
    session: Session;
    headers: Record<string, string>;
    body: unknown;
}

export class AuthMiddleware {
    private authService: AuthService;

    constructor(authService: AuthService) {
        this.authService = authService;
    }

    /**
     * Middleware function that verifies the Authorization header token.
     */
    authenticate(req: AuthenticatedRequest): boolean {
        const token = req.headers['authorization'];
        if (!token) {
            logger.warn('No authorization token provided');
            return false;
        }

        const session = this.authService.verifyToken(token);
        if (!session) {
            logger.warn('Invalid token');
            return false;
        }

        req.session = session;
        logger.info(`Authenticated user ${session.userId}`);
        return true;
    }

    /**
     * Check if a user has admin privileges.
     */
    requireAdmin(req: AuthenticatedRequest): boolean {
        if (!this.authenticate(req)) {
            return false;
        }
        // Check admin role
        return req.user?.email?.endsWith('@admin.com') ?? false;
    }
}
