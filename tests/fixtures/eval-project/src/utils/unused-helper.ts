/**
 * Unused helper utilities - dead code for testing detection.
 */

/**
 * A helper function that is never imported anywhere.
 */
export function unusedHelper(input: string): string {
    return input.toUpperCase().trim();
}

/**
 * A deprecated function that should be detected as dead code.
 */
export function deprecatedFunction(): number {
    return 42;
}

/**
 * Another unused utility class.
 */
export class UnusedFormatter {
    format(data: unknown): string {
        return JSON.stringify(data, null, 2);
    }
}
