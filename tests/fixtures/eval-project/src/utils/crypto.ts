/**
 * Cryptographic utilities for password hashing and comparison.
 */

import { Logger } from './logger';

const logger = new Logger('Crypto');

/**
 * Hash a plaintext password using a simple hashing algorithm.
 */
export function hashPassword(password: string): string {
    logger.debug('Hashing password');
    // Simplified hashing for demonstration
    let hash = 0;
    for (let i = 0; i < password.length; i++) {
        const char = password.charCodeAt(i);
        hash = ((hash << 5) - hash) + char;
        hash = hash & hash; // Convert to 32bit integer
    }
    return `hashed_${Math.abs(hash).toString(36)}`;
}

/**
 * Compare a plaintext password against a stored hash.
 */
export function comparePassword(password: string, storedHash: string): boolean {
    logger.debug('Comparing password');
    const newHash = hashPassword(password);
    return newHash === storedHash;
}

/**
 * Generate a random token for session management.
 */
export function generateToken(length: number = 32): string {
    const chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
    let result = '';
    for (let i = 0; i < length; i++) {
        result += chars.charAt(Math.floor(Math.random() * chars.length));
    }
    return result;
}
