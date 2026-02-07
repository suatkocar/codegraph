/**
 * User repository for database operations on user records.
 */

import { Database } from './connection';
import { User } from '../auth/types';
import { Logger } from '../utils/logger';

const logger = new Logger('UserRepository');

export class UserRepository {
    private db: Database;

    constructor(db: Database) {
        this.db = db;
    }

    /**
     * Find a user by their unique ID.
     */
    async findById(id: string): Promise<User | null> {
        logger.info(`Finding user by id: ${id}`);
        const result = await this.db.query(
            'SELECT * FROM users WHERE id = ?',
            [id]
        );
        if (result.rowCount === 0) return null;
        return result.rows[0] as unknown as User;
    }

    /**
     * Find a user by their email address.
     */
    async findByEmail(email: string): Promise<User | null> {
        logger.info(`Finding user by email: ${email}`);
        const result = await this.db.query(
            'SELECT * FROM users WHERE email = ?',
            [email]
        );
        if (result.rowCount === 0) return null;
        return result.rows[0] as unknown as User;
    }

    /**
     * Create a new user record in the database.
     */
    async create(user: Omit<User, 'id' | 'createdAt'>): Promise<User> {
        logger.info(`Creating user: ${user.email}`);
        const result = await this.db.query(
            'INSERT INTO users (email, name, password_hash) VALUES (?, ?, ?) RETURNING *',
            [user.email, user.name, user.passwordHash]
        );
        return result.rows[0] as unknown as User;
    }

    /**
     * Delete a user by their ID.
     */
    async deleteById(id: string): Promise<boolean> {
        logger.info(`Deleting user: ${id}`);
        const result = await this.db.query(
            'DELETE FROM users WHERE id = ?',
            [id]
        );
        return result.rowCount > 0;
    }
}
