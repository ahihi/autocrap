import AppKit

/// What the supervised autocrap process is currently doing.
enum AutocrapState {
    case notStarted
    case waitingForDevice
    case connected
    case failed(String)
}

/// A config file offered in the menu.
struct SelectableConfig {
    let url: URL
    let label: String
}

/// Supervises the autocrap process and reports what it is doing in the menu bar.
///
/// This owns no device logic. It starts a process, reads its two output streams and
/// draws a menu; everything about USB, MIDI, OSC and banking stays in the Rust binary,
/// which remains fully usable on its own from a terminal.
final class AutocrapController: NSObject, NSApplicationDelegate {
    private static let rememberedConfigKey = "SelectedConfigFileName"
    private static let statusSentinel = "@autocrap"
    private static let shutdownGracePeriod: TimeInterval = 3.0
    private static let largestLogBeforeTruncating = 4 * 1024 * 1024

    private let statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)

    private var state: AutocrapState = .notStarted
    private var activeBankIndex: Int?
    private var bankCount = 0

    private var autocrapProcess: Process?
    private var pendingStatusText = ""

    private lazy var repositoryRoot = Bundle.main.bundleURL.deletingLastPathComponent()
    private lazy var autocrapBinary = repositoryRoot
        .appendingPathComponent("target/release/autocrap")
    private lazy var configDirectory = repositoryRoot.appendingPathComponent("config")
    private lazy var calibrationFile = repositoryRoot.appendingPathComponent("calibration.json")
    private lazy var logFile = FileManager.default
        .homeDirectoryForCurrentUser
        .appendingPathComponent("Library/Logs/autocrap.log")

    private var availableConfigs: [SelectableConfig] = []
    private var selectedConfig: SelectableConfig?

    // MARK: - Lifecycle

    func applicationDidFinishLaunching(_ notification: Notification) {
        // two supervisors competing for one USB device is worse than a confusing no-op
        if isAlreadyRunningElsewhere() {
            NSApp.terminate(nil)
            return
        }

        truncateLogIfItHasGrownTooLarge()
        availableConfigs = discoverConfigs()
        selectedConfig = rememberedConfig() ?? availableConfigs.first

        redraw()
        startAutocrap()
    }

    func applicationWillTerminate(_ notification: Notification) {
        stopAutocrap()
    }

    private func isAlreadyRunningElsewhere() -> Bool {
        guard let identifier = Bundle.main.bundleIdentifier else { return false }
        return NSRunningApplication
            .runningApplications(withBundleIdentifier: identifier)
            .filter { !$0.isTerminated }
            .count > 1
    }

    // MARK: - Configs

    private func discoverConfigs() -> [SelectableConfig] {
        let files = (try? FileManager.default.contentsOfDirectory(
            at: configDirectory, includingPropertiesForKeys: nil
        )) ?? []

        return files
            .filter { $0.pathExtension == "json" }
            .sorted { $0.lastPathComponent < $1.lastPathComponent }
            .map { SelectableConfig(url: $0, label: label(forConfigAt: $0)) }
    }

    /// The config's own `name`, falling back to its filename when it has none.
    private func label(forConfigAt url: URL) -> String {
        if let data = try? Data(contentsOf: url),
           let parsed = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
           let name = parsed["name"] as? String, !name.isEmpty {
            return name
        }
        return url.deletingPathExtension().lastPathComponent
    }

    private func rememberedConfig() -> SelectableConfig? {
        guard let remembered = UserDefaults.standard.string(forKey: Self.rememberedConfigKey)
        else { return nil }
        // the remembered file may have been renamed or deleted since last launch
        return availableConfigs.first { $0.url.lastPathComponent == remembered }
    }

    // MARK: - Running autocrap

    private func startAutocrap() {
        guard FileManager.default.isExecutableFile(atPath: autocrapBinary.path) else {
            state = .failed("autocrap has not been built — run packaging/macos/build.sh")
            redraw()
            return
        }

        guard let config = selectedConfig else {
            state = .failed("no configs found in \(configDirectory.path)")
            redraw()
            return
        }

        var arguments = ["-c", config.url.path, "--status"]

        // corrections for this device's faults are not part of the config being
        // switched, so they are applied whichever one is selected
        if FileManager.default.fileExists(atPath: calibrationFile.path) {
            arguments += ["--calibration", calibrationFile.path]
        }

        let process = Process()
        process.executableURL = autocrapBinary
        process.arguments = arguments
        process.currentDirectoryURL = repositoryRoot

        let statusPipe = Pipe()
        let logPipe = Pipe()
        process.standardOutput = statusPipe
        process.standardError = logPipe

        statusPipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let chunk = handle.availableData
            guard !chunk.isEmpty, let text = String(data: chunk, encoding: .utf8) else { return }
            DispatchQueue.main.async { self?.consumeStatusOutput(text) }
        }

        logPipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let chunk = handle.availableData
            guard !chunk.isEmpty else { return }
            self?.appendToLog(chunk)
        }

        process.terminationHandler = { [weak self] finished in
            DispatchQueue.main.async {
                guard let self else { return }
                // a process that has already been replaced must not report its own
                // exit, or it would wipe out the state of the successor that
                // replaced it and leave that successor unsupervised
                guard self.autocrapProcess === finished else { return }
                self.autocrapDidExit(status: finished.terminationStatus)
            }
        }

        do {
            try process.run()
        } catch {
            log("could not start autocrap: \(error.localizedDescription)")
            state = .failed("could not start autocrap: \(error.localizedDescription)")
            redraw()
            return
        }

        autocrapProcess = process
        let calibrated = FileManager.default.fileExists(atPath: calibrationFile.path)
        log("started autocrap (pid \(process.processIdentifier)) with config \(config.label)"
            + (calibrated ? " and device calibration" : ""))
        state = .waitingForDevice
        activeBankIndex = nil
        bankCount = 0
        redraw()
    }

    private func stopAutocrap() {
        guard let process = autocrapProcess else { return }
        autocrapProcess = nil

        (process.standardOutput as? Pipe)?.fileHandleForReading.readabilityHandler = nil
        (process.standardError as? Pipe)?.fileHandleForReading.readabilityHandler = nil

        guard process.isRunning else { return }
        log("stopping autocrap (pid \(process.processIdentifier))")

        // SIGTERM rather than SIGKILL: it is what lets autocrap clear the device's
        // lights and release the USB interface, without which the device needs a
        // physical replug before anything can talk to it again
        process.terminate()

        let deadline = Date().addingTimeInterval(Self.shutdownGracePeriod)
        while process.isRunning && Date() < deadline {
            Thread.sleep(forTimeInterval: 0.05)
        }

        if process.isRunning {
            log("autocrap did not stop in time, killing it")
            kill(process.processIdentifier, SIGKILL)
        }
    }

    private func autocrapDidExit(status: Int32) {
        // deliberately not restarted: a crash loop would be invisible here and would
        // hammer the device. restarting is a menu action the user takes knowingly.
        autocrapProcess = nil
        activeBankIndex = nil

        log("autocrap exited unexpectedly with status \(status)")
        if case .failed = state {} else {
            state = .failed("autocrap stopped (exit \(status)) — see the log")
        }
        redraw()
    }

    // MARK: - Status stream

    private func consumeStatusOutput(_ text: String) {
        pendingStatusText += text

        while let lineBreak = pendingStatusText.firstIndex(of: "\n") {
            let line = String(pendingStatusText[..<lineBreak])
            pendingStatusText = String(pendingStatusText[pendingStatusText.index(after: lineBreak)...])
            apply(statusLine: line)
        }
    }

    private func apply(statusLine line: String) {
        // anything without the sentinel is incidental output, not a status event
        guard line.hasPrefix(Self.statusSentinel) else { return }

        let event = line.dropFirst(Self.statusSentinel.count).trimmingCharacters(in: .whitespaces)
        // values may contain spaces, so only the first separator is significant
        guard let separator = event.firstIndex(of: "=") else { return }

        let key = String(event[..<separator])
        let value = String(event[event.index(after: separator)...])

        switch key {
        case "device":
            state = (value == "connected") ? .connected : .waitingForDevice
        case "bank":
            activeBankIndex = Int(value)
        case "banks":
            bankCount = Int(value) ?? 0
        case "config":
            break
        default:
            // unknown keys are ignored so autocrap can add events without breaking this
            break
        }

        redraw()
    }

    // MARK: - Log

    /// Records what the app itself did, so its actions can be read alongside
    /// autocrap's own output rather than being invisible.
    private func log(_ message: String) {
        appendToLog(Data("[app] \(message)\n".utf8))
    }

    private func appendToLog(_ chunk: Data) {
        let manager = FileManager.default
        if !manager.fileExists(atPath: logFile.path) {
            try? manager.createDirectory(
                at: logFile.deletingLastPathComponent(), withIntermediateDirectories: true
            )
            manager.createFile(atPath: logFile.path, contents: nil)
        }

        guard let handle = try? FileHandle(forWritingTo: logFile) else { return }
        defer { try? handle.close() }
        _ = try? handle.seekToEnd()
        try? handle.write(contentsOf: chunk)
    }

    private func truncateLogIfItHasGrownTooLarge() {
        // a diagnostic aid rather than an archive, so old output is not worth keeping
        let size = (try? FileManager.default.attributesOfItem(atPath: logFile.path)[.size]) as? Int
        if let size, size > Self.largestLogBeforeTruncating {
            try? Data().write(to: logFile)
        }
    }

    // MARK: - Menu

    /// One colour per bank above the first, so the active bank is readable at a glance.
    ///
    /// Bank 0 is deliberately absent: it uses the menu bar's own colour instead, which
    /// is the resting state and should not compete for attention. Red is reserved for
    /// the failure glyph, so no bank may use it.
    private static let bankTints: [NSColor] = [.systemOrange, .systemGreen, .systemBlue]

    private func redraw() {
        statusItem.button?.image = glyphForCurrentState()
        statusItem.menu = buildMenu()
    }

    private func glyphForCurrentState() -> NSImage? {
        switch state {
        case .connected:
            return glyph("dial.medium.fill", or: "circle.fill", tintedBy: tintForActiveBank())
        case .waitingForDevice:
            // shape carries the connection state, colour carries the bank, so the
            // bank stays readable even while the device is away
            return glyph("dial.medium", or: "circle.dotted", tintedBy: tintForActiveBank())
        case .notStarted, .failed:
            return glyph("exclamationmark.triangle", or: "circle.slash", tintedBy: .systemRed)
        }
    }

    /// The active bank's colour, or `nil` for bank 0 and for a config without banking.
    ///
    /// `nil` means draw as a template, which the menu bar renders white on a dark bar
    /// and black on a light one, rather than hardcoding a colour that would vanish
    /// against one of them.
    private func tintForActiveBank() -> NSColor? {
        guard bankCount > 0, let activeBankIndex, activeBankIndex > 0 else { return nil }
        return Self.bankTints[(activeBankIndex - 1) % Self.bankTints.count]
    }

    private func glyph(_ name: String, or fallback: String, tintedBy tint: NSColor?) -> NSImage? {
        guard let symbol = NSImage(systemSymbolName: name, accessibilityDescription: nil)
            ?? NSImage(systemSymbolName: fallback, accessibilityDescription: nil)
        else { return nil }

        let sized = symbol.withSymbolConfiguration(
            NSImage.SymbolConfiguration(pointSize: 15, weight: .regular)
        ) ?? symbol

        guard let tint else {
            sized.isTemplate = true
            return sized
        }

        let coloured = NSImage(size: sized.size, flipped: false) { bounds in
            sized.draw(in: bounds)
            tint.set()
            bounds.fill(using: .sourceAtop)
            return true
        }
        coloured.isTemplate = false
        return coloured
    }

    private func buildMenu() -> NSMenu {
        let menu = NSMenu()

        menu.addItem(disabledItem(withTitle: statusDescription()))

        // hidden rather than greyed when banking is not configured: a permanently
        // empty row reads as something being broken
        if bankCount > 0, let activeBankIndex {
            menu.addItem(disabledItem(
                withTitle: "Bank \(activeBankIndex + 1) of \(bankCount)"
            ))
        }

        menu.addItem(.separator())
        menu.addItem(configMenuItem())
        menu.addItem(.separator())

        menu.addItem(actionItem(title: "Open Log", action: #selector(openLog)))
        menu.addItem(actionItem(title: "Restart", action: #selector(restart)))
        menu.addItem(.separator())
        menu.addItem(actionItem(title: "Quit Autocrap", action: #selector(quit), key: "q"))

        return menu
    }

    private func statusDescription() -> String {
        switch state {
        case .connected: return "Nocturn connected"
        case .waitingForDevice: return "Waiting for device"
        case .notStarted: return "Not running"
        case .failed(let reason): return reason
        }
    }

    private func configMenuItem() -> NSMenuItem {
        let item = NSMenuItem(title: "Config", action: nil, keyEquivalent: "")
        let submenu = NSMenu()

        if availableConfigs.isEmpty {
            submenu.addItem(disabledItem(withTitle: "none found"))
        }

        for (index, config) in availableConfigs.enumerated() {
            let entry = NSMenuItem(
                title: config.label, action: #selector(selectConfig(_:)), keyEquivalent: ""
            )
            entry.target = self
            entry.tag = index
            entry.state = (config.url == selectedConfig?.url) ? .on : .off
            submenu.addItem(entry)
        }

        item.submenu = submenu
        return item
    }

    private func disabledItem(withTitle title: String) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
        item.isEnabled = false
        return item
    }

    private func actionItem(title: String, action: Selector, key: String = "") -> NSMenuItem {
        let item = NSMenuItem(title: title, action: action, keyEquivalent: key)
        item.target = self
        return item
    }

    // MARK: - Menu actions

    @objc private func selectConfig(_ sender: NSMenuItem) {
        guard sender.tag < availableConfigs.count else { return }
        let chosen = availableConfigs[sender.tag]
        guard chosen.url != selectedConfig?.url else { return }

        selectedConfig = chosen
        UserDefaults.standard.set(chosen.url.lastPathComponent, forKey: Self.rememberedConfigKey)
        log("switching to config \(chosen.label)")

        stopAutocrap()
        startAutocrap()
    }

    @objc private func openLog() {
        NSWorkspace.shared.open(logFile)
    }

    @objc private func restart() {
        stopAutocrap()
        availableConfigs = discoverConfigs()
        startAutocrap()
    }

    @objc private func quit() {
        NSApp.terminate(nil)
    }
}

let application = NSApplication.shared
let controller = AutocrapController()
application.delegate = controller
application.setActivationPolicy(.accessory)
application.run()
