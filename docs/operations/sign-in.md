# Who may sign in

On a Windows PC, the Analyte app signs you in with your Windows account, so
you do not type the admin key (ADR 0021, Windows sign-in).

## What you see in the app

- **Opening the app:** the pages open already signed in. The foot of the
  menu says **Signed in as** `PC-NAME\name`.
- **Signing out:** **Sign out** brings you to the sign-in page, and you stay
  signed out until you choose how to sign in again.
- **Signing in again:** **Sign in with Windows** on the sign-in page signs you
  in with your Windows account at once. The admin key also works there.
- **After a restart of Analyte, or after an hour:** sessions end then. The
  next page you open signs you in again by itself and opens that page.
- **Not allowed:** if your Windows account may not use Analyte, the sign-in
  page says so in one sentence, with the admin key's field below it.
- **Windows sign-in not working:** if Analyte has no Windows sign-in (see
  Settings) or does not answer within a few seconds, the app shows the plain
  sign-in page with the admin key's field. The app signs in by itself at
  most once every 30 seconds, so it never keeps reloading; **Sign in with
  Windows** tries again whenever you click it.
- **Analyte stopped:** the app shows its Engine off screen, as before, and
  signs in by itself once Analyte answers again.

## Who is allowed

- Members of the Windows group **Analyte users** on this PC.
- The PC's administrators.

Everyone else sees the normal sign-in form and needs the admin key.

Setup creates the group and puts in the person who ran setup. Running setup
again, for example to upgrade, keeps everyone already in it. Uninstalling
Analyte leaves the group and its members, because it may list other people;
remove the group yourself once nobody needs it.

## Add or remove someone

You need to be an administrator on the PC.

- **Windows Pro or Enterprise:** open **Computer Management**, then **Local
  Users and Groups** → **Groups** → **Analyte users**, and choose **Add**.
- **Windows Home** has no Local Users and Groups. Open Command Prompt or
  PowerShell as administrator and run:

  ```text
  net localgroup "Analyte users" <name> /add
  ```

  `<name>` is the person's Windows account, such as `maria`, or `LAB\maria`
  on a PC in a domain. Use `/delete` instead of `/add` to remove someone, and
  `net localgroup "Analyte users"` to see who is in it.

## When a change counts

A change counts from that person's **next Windows sign-in**. Windows fixes a
person's groups when they sign in, so they need to sign out of Windows and in
again; locking the screen is not enough. The same goes for removing someone,
and an Analyte session they already have open ends within an hour.

## Shared Windows accounts

If staff share one Windows account, for example "Lab", Analyte sees only that
account. The audit then shows that one name for everyone who used it. To tell
people apart, give each person their own Windows account.

## Remote Desktop

Someone connected through Remote Desktop signs in as themselves: as the
Windows account they used to connect. The same rule applies to them.

## Browsers

A browser such as Edge or Chrome keeps using the admin key, as before. The
admin key also stays the way back in if Windows sign-in is not possible.

The sign-in page has a **Sign in with Windows** button. In the Analyte app it
signs you in with your Windows account. In a browser it opens the same
sign-in page with the note "Windows sign-in works in the Analyte app" above
the admin key's field, so you sign in there with the key.

## Who is signed in

After a Windows sign-in, the foot of the menu says **Signed in as**
`PC-NAME\name`, and what you change is recorded under that name in the
history. If Windows could not give the name in time, the account's security
number (its SID, such as `S-1-5-21-…`) is shown instead. Signing in with the
admin key still says only **Signed in**, and changes read "admin session".

**Settings** shows one line, **Windows sign-in**: "On", or "Off because …"
with the reason, for example when another program held Windows sign-in's
channel when Analyte started, or when the Analyte users group is missing, so
only administrators can sign in with Windows. Analyte on Linux has no
Windows sign-in, and Settings shows no such line there.
