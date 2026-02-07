/**
 * Simple logger utility for application-wide logging.
 */

export enum LogLevel {
    DEBUG = 0,
    INFO = 1,
    WARN = 2,
    ERROR = 3,
}

export class Logger {
    private context: string;
    private level: LogLevel;

    constructor(context: string, level: LogLevel = LogLevel.INFO) {
        this.context = context;
        this.level = level;
    }

    debug(message: string): void {
        if (this.level <= LogLevel.DEBUG) {
            this.log('DEBUG', message);
        }
    }

    info(message: string): void {
        if (this.level <= LogLevel.INFO) {
            this.log('INFO', message);
        }
    }

    warn(message: string): void {
        if (this.level <= LogLevel.WARN) {
            this.log('WARN', message);
        }
    }

    error(message: string): void {
        this.log('ERROR', message);
    }

    private log(level: string, message: string): void {
        const timestamp = new Date().toISOString();
        console.log(`[${timestamp}] [${level}] [${this.context}] ${message}`);
    }
}
