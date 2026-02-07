/**
 * Authentication service handling login, logout, and token verification.
 */

import { User, Session, AuthConfig, AuthResult } from './types';
import { hashPassword, comparePassword } from '../utils/crypto';
import { Logger } from '../utils/logger';

const logger = new Logger('AuthService');

export class AuthService {
    private config: AuthConfig;
    private sessions: Map<string, Session>;

    constructor(config: AuthConfig) {
        this.config = config;
        this.sessions = new Map();
    }

    /**
     * Authenticate a user with email and password.
     */
    async login(email: string, password: string): Promise<AuthResult> {
        logger.info(`Login attempt for ${email}`);
        const hashedPassword = hashPassword(password);
        // In a real app, we'd look up the user from the database
        const user: User = {
            id: 'user-1',
            email,
            name: 'Test User',
            passwordHash: hashedPassword,
            createdAt: new Date(),
        };

        if (!comparePassword(password, user.passwordHash)) {
            logger.warn(`Failed login for ${email}`);
            return { success: false, error: 'Invalid credentials' };
        }

        const session = this.createSession(user);
        return { success: true, user, session };
    }

    /**
     * Log out a user by invalidating their session.
     */
    async logout(sessionId: string): Promise<void> {
        logger.info(`Logout session ${sessionId}`);
        this.sessions.delete(sessionId);
    }

    /**
     * Verify a JWT token and return the associated session.
     */
    verifyToken(token: string): Session | null {
        for (const session of this.sessions.values()) {
            if (session.token === token && session.expiresAt > new Date()) {
                return session;
            }
        }
        logger.warn('Token verification failed');
        return null;
    }

    private createSession(user: User): Session {
        const session: Session = {
            id: `sess-${Date.now()}`,
            userId: user.id,
            token: `jwt-${Math.random().toString(36)}`,
            expiresAt: new Date(Date.now() + this.config.tokenExpiry),
        };
        this.sessions.set(session.id, session);
        return session;
    }
}
