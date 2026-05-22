# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability in Marshal, please report it responsibly:

1. **Do NOT open a public GitHub issue**
2. Email security concerns to the maintainers
3. Include a detailed description of the vulnerability
4. Allow reasonable time for a fix before public disclosure

## Security Best Practices

### JWT Secret
- Must be at least 32 characters (256 bits)
- Generate with: `openssl rand -base64 48`
- Never commit to version control
- Rotate periodically

### Database
- Use strong passwords (20+ characters)
- Enable TLS for remote PostgreSQL connections
- Restrict network access to the database
- Use separate database users with minimal privileges

### Deployment
- Always use HTTPS in production (Caddy handles this automatically)
- Set `APP_DOMAIN` to your actual domain
- Configure reCAPTCHA to prevent bot registrations
- Set `SUPER_ADMIN_EMAILS` to restrict platform admin access
- Run containers as non-root (already configured in Dockerfiles)

### Environment Variables
- Never commit `.env` files
- Use secret management tools in production (Vault, AWS Secrets Manager, GCP Secret Manager)
- Rotate API keys and secrets regularly
