export const usedFunction = () => ({ value: 42 });

export const unusedFunction = () => 'not used anywhere';

export function anotherUnused(): void {
    // This function is exported but never imported
}
