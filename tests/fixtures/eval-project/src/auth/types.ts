/**
 * Core authentication types for the application.
 */

export interface User {
    id: string;
    email: string;
    name: string;
    passwordHash: string;
    createdAt: Date;
}

export interface Session {
    id: string;
    userId: string;
    token: string;
    expiresAt: Date;
}

export interface AuthConfig {
    jwtSecret: string;
    tokenExpiry: number;
    maxSessions: number;
}

export type AuthResult = {
    success: boolean;
    user?: User;
    session?: Session;
    error?: string;
};
