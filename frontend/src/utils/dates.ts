const startOfDay = (date: Date): number => new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
export const daysBefore = (date: Date, from: Date = new Date()): number => Math.round((startOfDay(from) - startOfDay(date)) / 86_400_000);
export const isSameDay = (a: Date, b: Date): boolean => startOfDay(a) === startOfDay(b);
