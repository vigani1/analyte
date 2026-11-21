# Who may sign in

On a Windows PC, the Analyte app can sign you in with your Windows account, so
you do not type the admin key (ADR 0021, Windows sign-in). The app's part of
this is still being built; setup already prepares the group below, so the list
of people is ready when it arrives.

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
