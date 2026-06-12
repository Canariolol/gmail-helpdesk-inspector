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

## Fix `Error 403: access_denied`

If Google shows:

```text
Access blocked: gmail-helpdesk-inspector has not completed the Google verification process
Error 403: access_denied
```

the app is still in testing and the Gmail account trying to sign in is not an
approved test user.

Fix it in Google Cloud Console:

1. Go to **APIs & Services**.
2. Open **OAuth consent screen**.
3. Open **Audience**.
4. In **Test users**, add the exact Gmail address that will sign in.
5. Save, wait a minute, then retry login.

For this MVP, do not publish the app to production unless you are ready for
Google OAuth verification for Gmail restricted scopes.
