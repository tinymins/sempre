package cli

const usage = `Sempre - cross-platform lifecycle manager for proxy cores

Main entry points:
  sempre install [--core <reference>] [--subscription <URL>|--subscription-file <path>] [--ui <source>] [--ui-sha256 <digest>] [--yes]
  sempre bundle export <directory>
  sempre bundle restore [--yes]
  sempre uninstall [--purge]
  sempre open
  sempre portable run

Web and UI:
  sempre web <status|listen|password>
  sempre ui <status|install|update|remove>
  sempre runtime <status|start|stop|restart|overview|capabilities|config|proxies|providers|rules|connections|dns|events|reload>

Core versions:
  sempre core list
  sempre core install <core[:owner/repository][@stable|@version]>
  sempre core update [core[:owner/repository]@stable]
  sempre core use <core[:owner/repository]@stable|core[:owner/repository]@version>
  sempre core remove <core[:owner/repository]@stable|core[:owner/repository]@version>
  sempre core current
  sempre run [--core core[:owner/repository]@stable|core[:owner/repository]@version]

Configuration:
  sempre subscription <list|create|show|save|use|remove|update|render|debug>
  sempre subscription source <add-url|add-raw|remove|test>
  sempre subscription set <http-or-https-url>
  sempre subscription schedule <duration|off>
  sempre subscription auto-restart <true|false>
  sempre subscription status
  sempre subscription clear
  sempre custom-node <list|add|update|remove>
  sempre config import <file>
  sempre update

Service and diagnostics:
  sempre service <install|uninstall|start|stop|restart|status>
  sempre service deploy <all|core|bin|data>   Portable mode only
  sempre status
  sempre logs [--follow]
  sempre doctor
  sempre version

Modes:
  sempre --system <command>       Use protected machine-wide data (default)
  sempre --portable <command>     Use .sempre beside the executable
  sempre portable enable|disable Manage the .sempre-portable marker

Core mutation commands accept --yes to restart a running managed core without
prompting, or --no-restart to save the change without restarting it.
Subscription profile changes are always staged without an interactive restart.
bundle install remains a deprecated, configuration-preserving alias for
install; use bundle restore only to replace system data from a snapshot.
`
