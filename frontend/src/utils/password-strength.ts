export interface PasswordStrength {
    score: number
    label: string
}

export const passwordStrength = (password: string): PasswordStrength => {
    if (!password) return {score: 0, label: 'empty'}

    let score = 0
    if (password.length >= 8) score++
    if (password.length >= 12) score++
    const classes = [/[a-z]/, /[A-Z]/, /\d/, /[^A-Za-z0-9]/].filter((re) => re.test(password)).length
    if (classes >= 2) score++
    if (classes >= 3) score++

    score = Math.min(4, score)
    const labels = ['very weak', 'weak', 'fair', 'good', 'strong']
    return {score, label: labels[score]}
};
