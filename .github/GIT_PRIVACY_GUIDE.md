# Git Privacy Guide

## ⚠️ Important: Protecting Your Privacy in Git

This repository has been configured to protect your personal information. Please follow these guidelines to maintain privacy.

## ✅ Current Configuration

Your git is configured with:
- **Name**: `HiderWild`
- **Email**: `noreply@github.com`

## 🔒 Pre-commit Hook

A pre-commit hook has been installed at `.git/hooks/pre-commit` that will:
- ✅ Check if you're using sensitive email or username
- ✅ Scan staged files for sensitive information
- ✅ Block commits that contain sensitive data

## 🚫 Avoid These

**Never commit with:**
- Real names (e.g., YourRealName)
- Personal email addresses (e.g., yourpersonal@email.com)
- Computer names (e.g., YOUR-COMPUTER-NAME)
- Phone numbers
- Physical addresses
- API keys or passwords

## ✅ Best Practices

1. **Check your git config before each commit:**
   ```bash
   git config user.name
   git config user.email
   ```

2. **If configuration is wrong, update it:**
   ```bash
   git config user.name "HiderWild"
   git config user.email "noreply@github.com"
   ```

3. **Use GitHub's noreply email:**
   - Find yours at: https://github.com/settings/emails
   - Format: `USERNAME@users.noreply.github.com`

4. **Review changes before committing:**
   ```bash
   git diff --staged
   ```

5. **Check commit history:**
   ```bash
   git log --format="%an <%ae> - %s" -5
   ```

## 🛠️ If You Accidentally Commit Sensitive Info

**DO NOT PUSH** to the remote repository yet!

1. **If you haven't pushed:**
   ```bash
   git reset --soft HEAD~1  # Undo last commit, keep changes
   # or
   git commit --amend       # Edit last commit
   ```

2. **If you've already pushed:**
   - Contact repository maintainer immediately
   - History rewrite will be required (destructive operation)

## 🔍 Regular Audits

Run this command periodically to check for sensitive info:
```bash
git log --all --format="%ae|%an" | sort | uniq
```

Expected output should only show:
- `noreply@github.com|HiderWild`
- Other public contributor emails

## 📚 Additional Resources

- [GitHub: Setting your commit email address](https://docs.github.com/en/account-and-profile/setting-up-and-managing-your-personal-account-on-github/managing-email-preferences/setting-your-commit-email-address)
- [GitHub: Keeping your email address private](https://docs.github.com/en/account-and-profile/setting-up-and-managing-your-personal-account-on-github/managing-email-preferences/setting-your-commit-email-address#about-commit-email-addresses)

## 🆘 Emergency Contact

If you discover sensitive information has been committed and pushed, open an issue immediately or contact the repository owner.

---

**Last Updated**: 2026-06-09
**Repository**: spacedrive-multimedia-manager
