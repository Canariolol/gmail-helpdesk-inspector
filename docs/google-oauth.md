# Google OAuth Setup

1. Create a Google Cloud project.
2. Configure an OAuth consent screen for private/testing use.
3. Create an OAuth web client.
4. Add this redirect URI:

   ```text
   http://localhost:8080/auth/google/callback
   ```

5. Put the client values in `.env`.

The app requests only the Gmail readonly scope:

```text
https://www.googleapis.com/auth/gmail.readonly
```

For public distribution, Gmail readonly is a restricted scope and may require
Google verification. This MVP targets private/local use.

