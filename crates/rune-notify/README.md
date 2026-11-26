# rune-notify

System notifications for Rune apps: `UNUserNotificationCenter` on macOS and iOS, toast
notifications on Windows, and the desktop's notification server (`org.freedesktop.Notifications`
on the session bus) on Linux. Elsewhere `available()` is false and every call does nothing.
Notifications are silent unless `Notification::sound(true)` asks for the system's sound.

On Linux the first `available()`, `post` or `remove` connects to the session bus and waits up to
about 2.5 s for a notification server, so make it after the first frame (`on_ready`). The
XDG portal (`org.freedesktop.portal.Notification`) is not used: a sandboxed app needs permission
to talk to `org.freedesktop.Notifications` (Flatpak: `--talk-name=org.freedesktop.Notifications`).
Buttons post where the server shows actions; text boxes (`Notification::reply`) are left out.

| Call | What it does |
| --- | --- |
| `register(&Identity::new(id, name).icon(path))` | Windows: gives the process its `AppUserModelID` (which also groups its windows on the taskbar) and registers the name and icon its toasts show, under `HKEY_CURRENT_USER\Software\Classes\AppUserModelId\<id>`. Call it first thing in `main`, before a window opens. macOS reads both from the bundle. Linux writes nothing: notifications carry the name as their app name and the id as their desktop entry (`<id>.desktop`, where the server finds the app's icon). |
| `adopt(&Identity)` | Windows: the same identity in the process, written nowhere, for an app whose installer registered the `AppUserModelId\<id>` key and the activation scheme: toasts post under that id and open that scheme. A second call with the same id only changes the scheme, so an app can add it once it knows its install. |
| `available()` | Whether the app can post: a bundled app on macOS and iOS, a registered one on Windows; on Linux, a notification server answers on the session bus. |
| `post(&Notification)`, `remove(ids)` | Posts a notification with an id, title, body and string user info, or takes it back out of the system's list. On macOS and iOS the first post asks for permission, and `remove` also calls off a post still waiting for the answer. `post` returns false when the app can't post, or the system refused the notification on the spot (Windows); later refusals come to `on_post_error`. |
| `Notification::thread(id, title)` | Groups notifications posted under one thread id, such as one conversation's. macOS and iOS stack them under one entry in Notification Center (`threadIdentifier`); Windows puts them under a header titled `title` (a toast `<header>`), and a click on the header comes back as a click on the notification. The Linux notification spec has no grouping, so servers show them as they show the app's others. |
| `Notification::sound(on)` | The system's notification sound with this notification; silent by default. macOS and iOS ask for permission to play sounds with the first post (an app the person already answered keeps their choice) and play none where sounds are off for the app. Windows plays its default toast sound; Linux asks the server for `message-new-instant`. |
| `Attachment::file(path)`, `Notification::attachment(a)` | A PNG, JPEG or GIF image, an AIFF, WAV, MP3 or M4A sound, or an MP4, MPEG or AVI video, its format read from its first bytes, within `attachment_limit(kind)`: 10 MB images (3 MB on Windows), 5 MB sounds, 50 MB videos. `Attachment::file` opens the file, so call it off the UI thread; it returns `AttachmentError::Unreadable`, `Unsupported` or `TooLarge`. macOS and iOS show every attachment from a copy in the temp folder (`UNNotificationAttachment` moves the file it is given), checked again when copied; one that can't be copied or that the system refuses is left out and the rest posts. Windows shows the first image in the toast and Linux hands it to the server as `image-path`: both read the app's file when the notification shows, so keep it until then. |
| `authorization(f)` | Calls `f` once, from any thread, with whether the person lets the app's notifications show: `NotDetermined`, `Denied`, `Authorized`, `Provisional` or `Ephemeral` on macOS and iOS (from the center's settings); on Windows `Denied` when the person or a policy turned the app's toasts off, `NotDetermined` before Windows has a record of them, else `Authorized`; on Linux `Authorized` when a server answers. `Unavailable` wherever `available()` is false. It never blocks; ask again when the app comes forward, since Settings can change it. |
| `on_post_error(f)` | Calls `f` with a `PostError` (the id and a `Failure`: `Denied`, or `System` with the system's description) for every notification `post` took that didn't show: on macOS and iOS a turned-down or failed permission request or a failed add, on Windows a toast posted while the app's toasts are off or one `Show` refused, on Linux a failed `Notify` call. A post `remove` called off, or one a later post of the same id replaced before the system answered, is not reported. Install it at launch. |
| `on_click(f)` | Calls `f` with the clicked notification's id and user info, from any thread. Install it at launch. On Windows a click reaches the process that posted the toast while it runs. On Linux a click reaches the process that posted while it runs, with the server's XDG activation token in `Click::activation_token()` when it sends one (a Wayland window needs it to take focus). |
| `Identity::activation(scheme)`, `click_from_url(url)` | Windows: toasts open `<scheme>://notification/<id>?<user info>` when clicked, so a toast left in the notification center starts the app after it quit. `register` points the scheme at the running executable for the user. Hand the links the app receives (`rune::App::on_open_url`, with `rune::single_instance` so a click while it runs reaches the running process) to `click_from_url`, which returns the click for the app's own links. |
| `app_focused()`, `bring_to_front()` | Whether one of the app's windows is in front (skip notifying then), and bringing the frontmost window forward and focusing it, restoring it if minimized, for a click. |

```rust
fn main() {
    let _ = rune_notify::register(&rune_notify::Identity::new("com.example.app", "Example").icon(icon_png));
    rune_notify::on_click(|click| open(click.get("session")));
    // Later, while the window isn't focused:
    rune_notify::post(&rune_notify::Notification::new("n-1", "Response ready").body("Fix the build").info("session", "ses_1"));
}
```

`examples/notify-threads` posts three notifications in one thread, the second with an image,
silent or with a sound on the first; it shows the app's permission and every post error, and
posts one and removes it at once to show that a post removed while it waits never appears.

`rune_mac::notify` re-exports this crate.
