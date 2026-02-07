/**
 * Application entry point.
 */

import { configureRoutes } from './api/routes';
import { AuthService } from './auth/service';
import { AuthMiddleware } from './auth/middleware';
import { Database } from './db/connection';
import { UserRepository } from './db/user-repo';
import { Logger } from './utils/logger';

const logger = new Logger('App');

async function main(): Promise<void> {
    logger.info('Starting application');

    // Initialize database
    const db = new Database('postgres://localhost:5432/myapp');
    await db.connect();

    // Initialize services
    const authService = new AuthService({
        jwtSecret: 'secret-key',
        tokenExpiry: 3600000,
        maxSessions: 10,
    });

    const authMiddleware = new AuthMiddleware(authService);
    const userRepo = new UserRepository(db);

    // Configure routes
    const routes = configureRoutes(authMiddleware, authService, userRepo);
    logger.info(`Configured ${routes.length} routes`);

    // Start server
    logger.info('Server running on port 3000');
}

main().catch((err) => {
    logger.error(`Fatal error: ${err}`);
    process.exit(1);
});
