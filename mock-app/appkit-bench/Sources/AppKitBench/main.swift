import AppKit
import Carbon
import Foundation

private let appName = "appkit"
private let cachedZedCommandURL = resolveZedCommand()

// MARK: - Config

struct ShortcutConfig: Codable {
  let modifiers: [String]
  let key: String

  static let `default` = ShortcutConfig(modifiers: ["control"], key: "m")
}

struct AppConfig: Codable {
  let projects: [Project]
  let shortcut: ShortcutConfig

  enum CodingKeys: String, CodingKey {
    case projects, shortcut
  }

  init(projects: [Project], shortcut: ShortcutConfig) {
    self.projects = projects
    self.shortcut = shortcut
  }

  init(from decoder: Decoder) throws {
    let c = try decoder.container(keyedBy: CodingKeys.self)
    projects = try c.decode([Project].self, forKey: .projects)
    shortcut = try c.decodeIfPresent(ShortcutConfig.self, forKey: .shortcut) ?? .default
  }

  static func load() -> AppConfig {
    let home = FileManager.default.homeDirectoryForCurrentUser
    let configURL = home.appendingPathComponent(".project-manager.json")
    if let data = try? Data(contentsOf: configURL),
      let config = try? JSONDecoder().decode(AppConfig.self, from: data)
    {
      return config
    }
    return AppConfig(projects: loadProjects(), shortcut: .default)
  }

  func save() throws {
    let home = FileManager.default.homeDirectoryForCurrentUser
    let configURL = home.appendingPathComponent(".project-manager.json")
    let tmpURL = home.appendingPathComponent(".project-manager.json.tmp")
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
    let data = try encoder.encode(self)
    try data.write(to: tmpURL, options: .atomic)
    if FileManager.default.fileExists(atPath: configURL.path) {
      try FileManager.default.removeItem(at: configURL)
    }
    try FileManager.default.moveItem(at: tmpURL, to: configURL)
  }
}

// MARK: - Project

struct Project: Decodable, Encodable {
  let id: String
  let name: String
  let path: String
  let openPaths: [String]
  let aliases: [String]
  let tags: [String]
  let language: String
  let lastOpenedAt: String

  enum CodingKeys: String, CodingKey {
    case id, name, path, openPaths, aliases, tags, language, lastOpenedAt
  }

  init(
    id: String,
    name: String,
    path: String,
    openPaths: [String] = [],
    aliases: [String] = [],
    tags: [String] = [],
    language: String = "Action",
    lastOpenedAt: String = "2026-06-03T00:00:00Z"
  ) {
    self.id = id
    self.name = name
    self.path = path
    self.openPaths = openPaths
    self.aliases = aliases
    self.tags = tags
    self.language = language
    self.lastOpenedAt = lastOpenedAt
  }

  init(from decoder: Decoder) throws {
    let c = try decoder.container(keyedBy: CodingKeys.self)
    id = try c.decode(String.self, forKey: .id)
    name = try c.decode(String.self, forKey: .name)
    path = try c.decode(String.self, forKey: .path)
    openPaths = try c.decodeIfPresent([String].self, forKey: .openPaths) ?? []
    aliases = try c.decodeIfPresent([String].self, forKey: .aliases) ?? []
    tags = try c.decode([String].self, forKey: .tags)
    language = try c.decode(String.self, forKey: .language)
    lastOpenedAt = try c.decodeIfPresent(String.self, forKey: .lastOpenedAt) ?? ""
  }
}

struct IndexedProject {
  let project: Project
  let id: String
  let name: String
  let path: String
  let aliases: String
  let aliasList: [String]
  let tags: String
}

struct SearchResult {
  let project: Project
  let score: Int
  let matchedAlias: String?
}

final class BenchLogger {
  private let handle: FileHandle

  init() {
    let dir = FileManager.default.homeDirectoryForCurrentUser
      .appendingPathComponent("Library/Logs/ProjectLauncherBench")
    try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    let file = dir.appendingPathComponent(
      "appkit-\(ProcessInfo.processInfo.processIdentifier).jsonl")
    FileManager.default.createFile(atPath: file.path, contents: nil)
    handle = (try? FileHandle(forWritingTo: file)) ?? FileHandle.standardError
  }

  func log(_ event: String, cycleID: String? = nil, fields: [String: Any] = [:]) {
    var payload = fields
    payload["app"] = appName
    payload["event"] = event
    payload["mono_ns"] = DispatchTime.now().uptimeNanoseconds
    if let cycleID {
      payload["cycle_id"] = cycleID
    }
    guard JSONSerialization.isValidJSONObject(payload),
      let data = try? JSONSerialization.data(withJSONObject: payload),
      let newline = "\n".data(using: .utf8)
    else {
      return
    }
    handle.seekToEndOfFile()
    handle.write(data)
    handle.write(newline)
  }
}

final class SearchEngine {
  private let indexed: [IndexedProject]
  private let aliasLookup: [String: IndexedProject]

  init(projects: [Project]) {
    let allProjects =
      [
        Project(
          id: "debug-switch-to-tauri",
          name: "Switch to TauriBench",
          path: "/Applications/TauriBench.app",
          aliases: ["-"],
          tags: ["debug", "switch"],
          language: "Action")
      ] + projects
    indexed = allProjects.map {
      let aliases = $0.aliases.map { $0.lowercased() }
      return IndexedProject(
        project: $0,
        id: $0.id.lowercased(),
        name: $0.name.lowercased(),
        path: $0.path.lowercased(),
        aliases: aliases.joined(separator: " "),
        aliasList: aliases,
        tags: $0.tags.joined(separator: " ").lowercased()
      )
    }
    var lookup: [String: IndexedProject] = [:]
    for item in indexed {
      for alias in item.aliasList where lookup[alias] == nil {
        lookup[alias] = item
      }
    }
    aliasLookup = lookup
  }

  func search(_ query: String, limit: Int = 50) -> [SearchResult] {
    let normalizedQuery = query.lowercased()
    if let aliasHit = aliasLookup[normalizedQuery] {
      return [SearchResult(project: aliasHit.project, score: 10_000, matchedAlias: normalizedQuery)]
    }

    let tokens =
      normalizedQuery
      .split(whereSeparator: \.isWhitespace)
      .map(String.init)

    if tokens.isEmpty {
      return indexed.prefix(limit).map {
        SearchResult(project: $0.project, score: 0, matchedAlias: nil)
      }
    }

    var results: [SearchResult] = []
    results.reserveCapacity(limit * 2)

    for item in indexed {
      var total = 0
      var matched = true

      for token in tokens {
        let score = scoreToken(token, item: item)
        if score == 0 {
          matched = false
          break
        }
        total += score
      }

      if matched {
        results.append(SearchResult(project: item.project, score: total, matchedAlias: nil))
      }
    }

    results.sort {
      if $0.score == $1.score {
        return $0.project.name < $1.project.name
      }
      return $0.score > $1.score
    }

    if results.count > limit {
      return Array(results.prefix(limit))
    }
    return results
  }

  private func scoreToken(_ token: String, item: IndexedProject) -> Int {
    if item.id == token { return 1400 }
    if item.aliasList.contains(token) { return 5000 }
    if token.count >= 3, item.id.contains(token) { return 1000 }
    if item.name.hasPrefix(token) { return 1200 - min(item.name.count, 300) }
    if token.count == 1, wordHasPrefix(token, item.name) { return 600 }
    if token.count >= 3, item.name.contains(token) { return 900 - min(item.name.count, 250) }
    if token.count >= 3, item.aliases.contains(token) { return 800 }
    if token.count >= 3, item.tags.contains(token) { return 700 }
    if token.count >= 3, item.path.contains(token) { return 450 }
    if token.contains(where: \.isNumber) { return 0 }
    if token.count >= 3, fuzzyContains(token, item.name) { return 250 }
    if token.count >= 3, fuzzyContains(token, item.path) { return 120 }
    return 0
  }

  private func fuzzyContains(_ token: String, _ candidate: String) -> Bool {
    var index = candidate.startIndex
    for char in token {
      guard let found = candidate[index...].firstIndex(of: char) else {
        return false
      }
      index = candidate.index(after: found)
    }
    return true
  }

  private func wordHasPrefix(_ token: String, _ candidate: String) -> Bool {
    candidate
      .split { !$0.isLetter && !$0.isNumber }
      .contains { $0.hasPrefix(token) }
  }
}

@MainActor
final class SearchField: NSTextField {
  var onEnter: (() -> Void)?
  var onEscape: (() -> Void)?
  var onAppendText: ((String) -> Void)?
  var onDeleteBackward: (() -> Void)?
  var onMoveSelection: ((Int) -> Void)?

  override var acceptsFirstResponder: Bool { true }

  override func keyDown(with event: NSEvent) {
    if event.keyCode == 36 {
      onEnter?()
      return
    }
    if event.keyCode == 53 {
      onEscape?()
      return
    }
    if event.keyCode == 51 {
      onDeleteBackward?()
      return
    }
    if event.keyCode == 125 {
      onMoveSelection?(1)
      return
    }
    if event.keyCode == 126 {
      onMoveSelection?(-1)
      return
    }
    let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
    if flags == .control, event.keyCode == 45 {
      onMoveSelection?(1)
      return
    }
    if flags == .control, event.keyCode == 35 {
      onMoveSelection?(-1)
      return
    }
    if flags.isDisjoint(with: [.command, .control, .option]),
      let ascii = asciiInput(from: event)
    {
      onAppendText?(ascii)
      return
    }
    super.keyDown(with: event)
  }
}

@MainActor
final class LauncherController: NSObject, NSTableViewDataSource, NSTableViewDelegate,
  NSTextFieldDelegate
{
  private let logger: BenchLogger
  private let searchEngine: SearchEngine
  private let panel: NSPanel
  private let searchField = SearchField()
  private let tableView = NSTableView()
  private let footerLabel = NSTextField(labelWithString: "")
  private var results: [SearchResult] = []
  private var activeCycleID: String?
  private var activeScenario = ""
  private var lastSearchQuery = ""
  private var queryValue = ""

  init(projects: [Project], logger: BenchLogger) {
    self.logger = logger
    self.searchEngine = SearchEngine(projects: projects)
    self.panel = NSPanel(
      contentRect: NSRect(x: 0, y: 0, width: 760, height: 520),
      styleMask: [.titled, .fullSizeContentView],
      backing: .buffered,
      defer: false
    )
    super.init()
    configurePanel()
    performSearch("", cycleID: nil)
  }

  func show(source: String) {
    let cycleID = "\(source)-\(UUID().uuidString)"
    activeCycleID = cycleID
    logger.log("hotkey_received", cycleID: cycleID, fields: ["source": source])
    setSearchQuery("")
    performSearch("", cycleID: cycleID)

    if let screen = NSScreen.main {
      let frame = panel.frame
      let visible = screen.visibleFrame
      panel.setFrameOrigin(
        NSPoint(
          x: visible.midX - frame.width / 2,
          y: visible.maxY - frame.height - 120
        ))
    }

    NSApp.activate(ignoringOtherApps: true)
    panel.makeKeyAndOrderFront(nil)
    panel.contentView?.layoutSubtreeIfNeeded()
    panel.contentView?.displayIfNeeded()
    panel.makeFirstResponder(searchField)

    DispatchQueue.main.async { [weak self] in
      guard let self else { return }
      self.panel.contentView?.layoutSubtreeIfNeeded()
      self.panel.contentView?.displayIfNeeded()
      self.logger.log("palette_rendered", cycleID: cycleID)
    }
  }

  func toggle(source: String) {
    if panel.isVisible {
      hide()
    } else {
      show(source: source)
    }
  }

  func hide() {
    panel.orderOut(nil)
    activeCycleID = nil
  }

  func handleBufferedKey(_ event: NSEvent) {
    guard panel.isVisible, !panel.isKeyWindow else { return }

    switch event.keyCode {
    case 36:
      openSelectedProject()
    case 53:
      hide()
    case 51:
      deleteSearchText()
    case 125:
      moveSelection(offset: 1)
    case 126:
      moveSelection(offset: -1)
    default:
      let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
      if flags == .control, event.keyCode == 45 {
        moveSelection(offset: 1)
        return
      }
      if flags == .control, event.keyCode == 35 {
        moveSelection(offset: -1)
        return
      }
      guard flags.isDisjoint(with: [.command, .control, .option]),
        let ascii = asciiInput(from: event)
      else {
        return
      }
      appendSearchText(ascii)
    }
  }

  func handleLocalKey(_ event: NSEvent) -> Bool {
    guard panel.isVisible else { return false }

    switch event.keyCode {
    case 36:
      openSelectedProject()
      return true
    case 53:
      hide()
      return true
    case 125:
      moveSelection(offset: 1)
      return true
    case 126:
      moveSelection(offset: -1)
      return true
    default:
      let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
      if flags == .control, event.keyCode == 45 {
        moveSelection(offset: 1)
        return true
      }
      if flags == .control, event.keyCode == 35 {
        moveSelection(offset: -1)
        return true
      }
      guard flags.isDisjoint(with: [.command, .control, .option]),
        let ascii = asciiInput(from: event)
      else {
        return false
      }
      appendSearchText(ascii)
      return true
    }
  }

  func runBenchmark() {
    let queries = ["a", "pr", "api", "web", "manager", "ios", "zed"]
    runBenchmarkStep(index: 0, total: 100, queries: queries)
  }

  private func runBenchmarkStep(index: Int, total: Int, queries: [String]) {
    guard index < total else {
      logger.log("benchmark_cycle_completed", fields: ["count": total])
      hide()
      return
    }

    show(source: "benchmark")
    let cycleID = activeCycleID
    setSearchQuery(queries[index % queries.count])
    performSearch(queryValue, cycleID: cycleID)

    DispatchQueue.main.asyncAfter(deadline: .now() + 0.012) { [weak self] in
      self?.hide()
      self?.runBenchmarkStep(index: index + 1, total: total, queries: queries)
    }
  }

  func numberOfRows(in tableView: NSTableView) -> Int {
    results.count
  }

  func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView?
  {
    let identifier = NSUserInterfaceItemIdentifier("ProjectCell")
    let textField: NSTextField
    if let existing = tableView.makeView(withIdentifier: identifier, owner: self) as? NSTextField {
      textField = existing
    } else {
      textField = NSTextField(labelWithString: "")
      textField.identifier = identifier
      textField.font = .systemFont(ofSize: 14, weight: .medium)
      textField.lineBreakMode = .byTruncatingMiddle
    }
    let project = results[row].project
    let aliasText = project.aliases.isEmpty ? "-" : project.aliases.joined(separator: ",")
    textField.stringValue =
      "\(project.name)  -  alias \(aliasText)  -  \(project.id)  -  \(project.language)  -  \(project.path)"
    return textField
  }

  func controlTextDidChange(_ obj: Notification) {
    guard panel.isVisible else { return }
    searchField.stringValue = queryValue
  }

  func control(_ control: NSControl, textView: NSTextView, doCommandBy commandSelector: Selector)
    -> Bool
  {
    switch commandSelector {
    case #selector(NSResponder.insertNewline(_:)):
      openSelectedProject()
      return true
    case #selector(NSResponder.cancelOperation(_:)):
      hide()
      return true
    case #selector(NSResponder.moveUp(_:)):
      moveSelection(offset: -1)
      return true
    case #selector(NSResponder.moveDown(_:)):
      moveSelection(offset: 1)
      return true
    default:
      return false
    }
  }

  private func configurePanel() {
    panel.titleVisibility = .hidden
    panel.titlebarAppearsTransparent = true
    panel.isReleasedWhenClosed = false
    panel.hidesOnDeactivate = false
    panel.level = .floating
    panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
    panel.isOpaque = false
    panel.backgroundColor = .clear

    let content = NSView()
    content.wantsLayer = true
    content.layer?.backgroundColor = NSColor.windowBackgroundColor.cgColor
    content.layer?.cornerRadius = 14
    content.layer?.masksToBounds = true
    panel.contentView = content

    searchField.translatesAutoresizingMaskIntoConstraints = false
    searchField.font = .systemFont(ofSize: 22, weight: .regular)
    searchField.placeholderString = "Search projects"
    searchField.isBezeled = true
    searchField.drawsBackground = true
    searchField.isEditable = false
    searchField.isSelectable = false
    searchField.delegate = self
    searchField.onEnter = { [weak self] in self?.openSelectedProject() }
    searchField.onEscape = { [weak self] in self?.hide() }
    searchField.onAppendText = { [weak self] value in self?.appendSearchText(value) }
    searchField.onDeleteBackward = { [weak self] in self?.deleteSearchText() }
    searchField.onMoveSelection = { [weak self] offset in self?.moveSelection(offset: offset) }

    let scrollView = NSScrollView()
    scrollView.translatesAutoresizingMaskIntoConstraints = false
    scrollView.hasVerticalScroller = true
    scrollView.borderType = .noBorder

    tableView.addTableColumn(NSTableColumn(identifier: NSUserInterfaceItemIdentifier("Project")))
    tableView.headerView = nil
    tableView.rowHeight = 34
    tableView.dataSource = self
    tableView.delegate = self
    tableView.usesAlternatingRowBackgroundColors = false
    scrollView.documentView = tableView

    footerLabel.translatesAutoresizingMaskIntoConstraints = false
    footerLabel.font = .monospacedSystemFont(ofSize: 11, weight: .regular)
    footerLabel.textColor = .secondaryLabelColor

    content.addSubview(searchField)
    content.addSubview(scrollView)
    content.addSubview(footerLabel)

    NSLayoutConstraint.activate([
      searchField.topAnchor.constraint(equalTo: content.topAnchor, constant: 20),
      searchField.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 20),
      searchField.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -20),
      searchField.heightAnchor.constraint(equalToConstant: 42),

      scrollView.topAnchor.constraint(equalTo: searchField.bottomAnchor, constant: 16),
      scrollView.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 20),
      scrollView.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -20),
      scrollView.bottomAnchor.constraint(equalTo: footerLabel.topAnchor, constant: -12),

      footerLabel.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 20),
      footerLabel.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -20),
      footerLabel.bottomAnchor.constraint(equalTo: content.bottomAnchor, constant: -14),
    ])
  }

  private func performSearch(_ query: String, cycleID: String?) {
    lastSearchQuery = query
    let start = DispatchTime.now().uptimeNanoseconds
    results = searchEngine.search(query)
    let end = DispatchTime.now().uptimeNanoseconds
    let scenario = scenarioName(query: query, aliasHit: results.first?.matchedAlias)
    activeScenario = scenario
    tableView.reloadData()
    if !results.isEmpty {
      tableView.selectRowIndexes(IndexSet(integer: 0), byExtendingSelection: false)
    }
    let duration = Double(end - start) / 1_000_000
    if let alias = results.first?.matchedAlias {
      footerLabel.stringValue =
        "alias \(alias) -> \(results[0].project.name) - search \(String(format: "%.3f", duration)) ms"
    } else {
      footerLabel.stringValue =
        "\(results.count) results - search \(String(format: "%.3f", duration)) ms"
    }
    logger.log(
      "search_completed", cycleID: cycleID,
      fields: [
        "metric": "search_ms",
        "duration_ms": duration,
        "query": query,
        "result_count": results.count,
        "alias_hit": results.first?.matchedAlias ?? "",
        "scenario": scenario,
      ])
  }

  private func appendSearchText(_ value: String) {
    let start = DispatchTime.now().uptimeNanoseconds
    setSearchQuery(queryValue + value)
    performSearch(queryValue, cycleID: activeCycleID)
    let duration = Double(DispatchTime.now().uptimeNanoseconds - start) / 1_000_000
    logger.log(
      "input_processed", cycleID: activeCycleID,
      fields: [
        "metric": "input_to_result_ms",
        "duration_ms": duration,
        "query": queryValue,
        "result_count": results.count,
        "scenario": activeScenario,
      ])
  }

  private func deleteSearchText() {
    guard !queryValue.isEmpty else { return }
    setSearchQuery(String(queryValue.dropLast()))
    performSearch(queryValue, cycleID: activeCycleID)
  }

  private func moveSelection(offset: Int) {
    guard !results.isEmpty else { return }
    let start = DispatchTime.now().uptimeNanoseconds
    let current = tableView.selectedRow >= 0 ? tableView.selectedRow : 0
    let next = max(0, min(results.count - 1, current + offset))
    tableView.selectRowIndexes(IndexSet(integer: next), byExtendingSelection: false)
    tableView.scrollRowToVisible(next)
    activeScenario = "navigation"
    let duration = Double(DispatchTime.now().uptimeNanoseconds - start) / 1_000_000
    logger.log(
      "selection_moved", cycleID: activeCycleID,
      fields: [
        "metric": "selection_move_ms",
        "duration_ms": duration,
        "query": queryValue,
        "selected_index": next,
        "scenario": activeScenario,
      ])
  }

  private func openSelectedProject() {
    let normalized = normalizeSearchQuery(queryValue)
    if normalized != queryValue { setSearchQuery(normalized) }
    if lastSearchQuery != normalized {
      performSearch(normalized, cycleID: activeCycleID)
    }
    guard !results.isEmpty else { return }
    let selected = tableView.selectedRow >= 0 ? tableView.selectedRow : 0
    let project = results[min(selected, results.count - 1)].project
    let cycleID = activeCycleID ?? "open-\(UUID().uuidString)"
    if project.id == "debug-switch-to-tauri" {
      switchToTauri(cycleID: cycleID)
      return
    }
    footerLabel.stringValue = "Opening \(project.name)..."
    logger.log("open_requested", cycleID: cycleID)

    let process = makeZedProcess(projectPaths: project.openPaths.isEmpty ? [project.path] : project.openPaths)
    do {
      try process.run()
      logger.log(
        "open_dispatched", cycleID: cycleID,
        fields: [
          "project_id": project.id,
          "scenario": activeScenario,
          "query": queryValue,
          "selected_index": selected,
        ])
    } catch {
      logger.log(
        "open_dispatch_failed", cycleID: cycleID,
        fields: [
          "project_id": project.id,
          "error": error.localizedDescription,
        ])
    }
    hide()
  }

  private func setSearchQuery(_ value: String) {
    queryValue = normalizeSearchQuery(value)
    searchField.stringValue = queryValue
  }

  private func switchToTauri(cycleID: String) {
    logger.log(
      "debug_switch_requested", cycleID: cycleID,
      fields: ["target": "TauriBench", "query": queryValue])
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/open")
    process.arguments = ["-a", "/Applications/TauriBench.app"]
    do {
      try process.run()
      logger.log("debug_switch_dispatched", cycleID: cycleID, fields: ["target": "TauriBench"])
    } catch {
      logger.log(
        "debug_switch_failed", cycleID: cycleID,
        fields: ["target": "TauriBench", "error": error.localizedDescription])
    }
    NSApp.terminate(nil)
  }
}

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
  private let logger = BenchLogger()
  private var launcher: LauncherController?
  private var statusItem: NSStatusItem?
  private var hotKeyRef: EventHotKeyRef?
  private var eventHandler: EventHandlerRef?
  private var globalKeyMonitor: Any?
  private var globalMouseMonitor: Any?
  private var localKeyMonitor: Any?
  private var config: AppConfig?
  private var settingsController: SettingsWindowController?

  func applicationDidFinishLaunching(_ notification: Notification) {
    NSApp.setActivationPolicy(.regular)
    let loaded = AppConfig.load()
    config = loaded
    launcher = LauncherController(projects: loaded.projects, logger: logger)
    configureStatusItem()
    registerHotKey(loaded.shortcut)
    registerGlobalKeyBuffer()
    registerGlobalMouseDismiss()
    registerLocalKeyCommands()
    logger.log("app_ready", fields: ["project_count": loaded.projects.count])

    // Show palette on first launch
    DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) { [weak self] in
      self?.launcher?.show(source: "launch")
    }
  }

  func applicationDidResignActive(_ notification: Notification) {
    launcher?.hide()
  }

  private func configureStatusItem() {
    let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
    item.button?.title = "A"

    let menu = NSMenu()
    menu.addItem(NSMenuItem(title: "Show", action: #selector(showFromMenu), keyEquivalent: ""))
    menu.addItem(NSMenuItem(title: "Settings...", action: #selector(openSettings), keyEquivalent: ","))
    menu.addItem(
      NSMenuItem(title: "Run Benchmark", action: #selector(runBenchmark), keyEquivalent: ""))
    menu.addItem(.separator())
    menu.addItem(NSMenuItem(title: "Quit", action: #selector(quit), keyEquivalent: "q"))
    item.menu = menu
    statusItem = item
  }

  @objc private func showFromMenu() {
    launcher?.show(source: "menu")
  }

  @objc private func runBenchmark() {
    launcher?.runBenchmark()
  }

  @objc private func openSettings() {
    if let controller = settingsController {
      controller.showWindow(nil)
      NSApp.activate(ignoringOtherApps: true)
      return
    }
    guard let config else { return }
    let controller = SettingsWindowController(config: config) { [weak self] newConfig in
      guard let self else { return }
      self.config = newConfig
      self.launcher = LauncherController(projects: newConfig.projects, logger: self.logger)
      self.reregisterHotKey(newConfig.shortcut)
    }
    settingsController = controller
    controller.showWindow(nil)
    NSApp.activate(ignoringOtherApps: true)
  }

  @objc private func quit() {
    NSApp.terminate(nil)
  }

  private func registerHotKey(_ shortcut: ShortcutConfig) {
    var eventSpec = EventTypeSpec(
      eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed))
    let selfPointer = Unmanaged.passUnretained(self).toOpaque()
    let callback: EventHandlerUPP = { _, _, userData in
      guard let userData else { return noErr }
      let delegate = Unmanaged<AppDelegate>.fromOpaque(userData).takeUnretainedValue()
      Task { @MainActor in
        delegate.launcher?.toggle(source: "hotkey")
      }
      return noErr
    }

    InstallEventHandler(
      GetApplicationEventTarget(), callback, 1, &eventSpec, selfPointer, &eventHandler)

    guard let keyCode = charToCarbonKeyCode(shortcut.key) else { return }
    let carbonFlags = modifierNamesToCarbonFlags(shortcut.modifiers)
    let hotKeyID = EventHotKeyID(signature: fourCharCode("APKB"), id: 1)
    RegisterEventHotKey(
      keyCode,
      carbonFlags,
      hotKeyID,
      GetApplicationEventTarget(),
      0,
      &hotKeyRef
    )
  }

  private func reregisterHotKey(_ shortcut: ShortcutConfig) {
    if let ref = hotKeyRef {
      UnregisterEventHotKey(ref)
      hotKeyRef = nil
    }
    guard let keyCode = charToCarbonKeyCode(shortcut.key) else { return }
    let carbonFlags = modifierNamesToCarbonFlags(shortcut.modifiers)
    let hotKeyID = EventHotKeyID(signature: fourCharCode("APKB"), id: 1)
    RegisterEventHotKey(
      keyCode,
      carbonFlags,
      hotKeyID,
      GetApplicationEventTarget(),
      0,
      &hotKeyRef
    )
  }

  private func registerGlobalKeyBuffer() {
    globalKeyMonitor = NSEvent.addGlobalMonitorForEvents(matching: .keyDown) { [weak self] event in
      Task { @MainActor in
        self?.launcher?.handleBufferedKey(event)
      }
    }
  }

  private func registerGlobalMouseDismiss() {
    globalMouseMonitor = NSEvent.addGlobalMonitorForEvents(
      matching: [.leftMouseDown, .rightMouseDown, .otherMouseDown]
    ) { [weak self] _ in
      Task { @MainActor in
        self?.launcher?.hide()
      }
    }
  }

  private func registerLocalKeyCommands() {
    localKeyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
      guard let self else { return event }
      if self.launcher?.handleLocalKey(event) == true {
        return nil
      }
      return event
    }
  }
}

private func scenarioName(query: String, aliasHit: String?) -> String {
  if aliasHit == "a" { return "alias" }
  if query.lowercased() == "pr" { return "narrowing" }
  return ""
}

private func fourCharCode(_ value: String) -> OSType {
  var result: OSType = 0
  for scalar in value.unicodeScalars.prefix(4) {
    result = (result << 8) + OSType(scalar.value)
  }
  return result
}

// MARK: - Shortcut key mapping

private func charToCarbonKeyCode(_ char: String) -> UInt32? {
  switch char.lowercased() {
  case "a": return UInt32(kVK_ANSI_A)
  case "b": return UInt32(kVK_ANSI_B)
  case "c": return UInt32(kVK_ANSI_C)
  case "d": return UInt32(kVK_ANSI_D)
  case "e": return UInt32(kVK_ANSI_E)
  case "f": return UInt32(kVK_ANSI_F)
  case "g": return UInt32(kVK_ANSI_G)
  case "h": return UInt32(kVK_ANSI_H)
  case "i": return UInt32(kVK_ANSI_I)
  case "j": return UInt32(kVK_ANSI_J)
  case "k": return UInt32(kVK_ANSI_K)
  case "l": return UInt32(kVK_ANSI_L)
  case "m": return UInt32(kVK_ANSI_M)
  case "n": return UInt32(kVK_ANSI_N)
  case "o": return UInt32(kVK_ANSI_O)
  case "p": return UInt32(kVK_ANSI_P)
  case "q": return UInt32(kVK_ANSI_Q)
  case "r": return UInt32(kVK_ANSI_R)
  case "s": return UInt32(kVK_ANSI_S)
  case "t": return UInt32(kVK_ANSI_T)
  case "u": return UInt32(kVK_ANSI_U)
  case "v": return UInt32(kVK_ANSI_V)
  case "w": return UInt32(kVK_ANSI_W)
  case "x": return UInt32(kVK_ANSI_X)
  case "y": return UInt32(kVK_ANSI_Y)
  case "z": return UInt32(kVK_ANSI_Z)
  case "0": return UInt32(kVK_ANSI_0)
  case "1": return UInt32(kVK_ANSI_1)
  case "2": return UInt32(kVK_ANSI_2)
  case "3": return UInt32(kVK_ANSI_3)
  case "4": return UInt32(kVK_ANSI_4)
  case "5": return UInt32(kVK_ANSI_5)
  case "6": return UInt32(kVK_ANSI_6)
  case "7": return UInt32(kVK_ANSI_7)
  case "8": return UInt32(kVK_ANSI_8)
  case "9": return UInt32(kVK_ANSI_9)
  case " ", "space": return UInt32(kVK_Space)
  default: return nil
  }
}

private func carbonKeyCodeToChar(_ keyCode: UInt16) -> String? {
  switch Int(keyCode) {
  case kVK_ANSI_A: return "a"
  case kVK_ANSI_B: return "b"
  case kVK_ANSI_C: return "c"
  case kVK_ANSI_D: return "d"
  case kVK_ANSI_E: return "e"
  case kVK_ANSI_F: return "f"
  case kVK_ANSI_G: return "g"
  case kVK_ANSI_H: return "h"
  case kVK_ANSI_I: return "i"
  case kVK_ANSI_J: return "j"
  case kVK_ANSI_K: return "k"
  case kVK_ANSI_L: return "l"
  case kVK_ANSI_M: return "m"
  case kVK_ANSI_N: return "n"
  case kVK_ANSI_O: return "o"
  case kVK_ANSI_P: return "p"
  case kVK_ANSI_Q: return "q"
  case kVK_ANSI_R: return "r"
  case kVK_ANSI_S: return "s"
  case kVK_ANSI_T: return "t"
  case kVK_ANSI_U: return "u"
  case kVK_ANSI_V: return "v"
  case kVK_ANSI_W: return "w"
  case kVK_ANSI_X: return "x"
  case kVK_ANSI_Y: return "y"
  case kVK_ANSI_Z: return "z"
  case kVK_ANSI_0: return "0"
  case kVK_ANSI_1: return "1"
  case kVK_ANSI_2: return "2"
  case kVK_ANSI_3: return "3"
  case kVK_ANSI_4: return "4"
  case kVK_ANSI_5: return "5"
  case kVK_ANSI_6: return "6"
  case kVK_ANSI_7: return "7"
  case kVK_ANSI_8: return "8"
  case kVK_ANSI_9: return "9"
  case kVK_Space: return "space"
  default: return nil
  }
}

private func modifierNamesToCarbonFlags(_ names: [String]) -> UInt32 {
  var flags: UInt32 = 0
  for name in names {
    switch name.lowercased() {
    case "control": flags |= UInt32(controlKey)
    case "option", "alt": flags |= UInt32(optionKey)
    case "command", "cmd", "super": flags |= UInt32(cmdKey)
    case "shift": flags |= UInt32(shiftKey)
    default: break
    }
  }
  return flags
}

private func carbonFlagsToModifierNames(_ flags: UInt32) -> [String] {
  var names: [String] = []
  if flags & UInt32(controlKey) != 0 { names.append("control") }
  if flags & UInt32(optionKey) != 0 { names.append("option") }
  if flags & UInt32(shiftKey) != 0 { names.append("shift") }
  if flags & UInt32(cmdKey) != 0 { names.append("command") }
  return names
}

private func asciiInput(from event: NSEvent) -> String? {
  if let characters = event.charactersIgnoringModifiers,
    let ascii = asciiPrintable(characters)
  {
    return ascii
  }

  switch Int(event.keyCode) {
  case kVK_ANSI_A: return "a"
  case kVK_ANSI_B: return "b"
  case kVK_ANSI_C: return "c"
  case kVK_ANSI_D: return "d"
  case kVK_ANSI_E: return "e"
  case kVK_ANSI_F: return "f"
  case kVK_ANSI_G: return "g"
  case kVK_ANSI_H: return "h"
  case kVK_ANSI_I: return "i"
  case kVK_ANSI_J: return "j"
  case kVK_ANSI_K: return "k"
  case kVK_ANSI_L: return "l"
  case kVK_ANSI_M: return "m"
  case kVK_ANSI_N: return "n"
  case kVK_ANSI_O: return "o"
  case kVK_ANSI_P: return "p"
  case kVK_ANSI_Q: return "q"
  case kVK_ANSI_R: return "r"
  case kVK_ANSI_S: return "s"
  case kVK_ANSI_T: return "t"
  case kVK_ANSI_U: return "u"
  case kVK_ANSI_V: return "v"
  case kVK_ANSI_W: return "w"
  case kVK_ANSI_X: return "x"
  case kVK_ANSI_Y: return "y"
  case kVK_ANSI_Z: return "z"
  case kVK_ANSI_0: return "0"
  case kVK_ANSI_1: return "1"
  case kVK_ANSI_2: return "2"
  case kVK_ANSI_3: return "3"
  case kVK_ANSI_4: return "4"
  case kVK_ANSI_5: return "5"
  case kVK_ANSI_6: return "6"
  case kVK_ANSI_7: return "7"
  case kVK_ANSI_8: return "8"
  case kVK_ANSI_9: return "9"
  case kVK_ANSI_Minus: return "-"
  case kVK_Space: return " "
  default: return nil
  }
}

private func asciiPrintable(_ value: String) -> String? {
  let scalars = value.unicodeScalars.filter { scalar in
    scalar.value >= 32 && scalar.value <= 126
  }
  guard !scalars.isEmpty else { return nil }
  return String(String.UnicodeScalarView(scalars))
}

private func normalizeSearchQuery(_ value: String) -> String {
  let mutable = NSMutableString(string: value)
  CFStringTransform(mutable, nil, kCFStringTransformFullwidthHalfwidth, false)
  CFStringTransform(mutable, nil, kCFStringTransformToLatin, false)
  CFStringTransform(mutable, nil, kCFStringTransformStripCombiningMarks, false)
  return mutable as String
}

private func makeZedProcess(projectPaths: [String]) -> Process {
  let process = Process()
  if let zedURL = cachedZedCommandURL {
    process.executableURL = zedURL
    process.arguments = projectPaths
  } else {
    process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
    process.arguments = ["zed"] + projectPaths
  }
  return process
}

private func resolveZedCommand() -> URL? {
  if let path = ProcessInfo.processInfo.environment["PATH"] {
    for directory in path.split(separator: ":").map(String.init) {
      let candidate = URL(fileURLWithPath: directory).appendingPathComponent("zed")
      if FileManager.default.isExecutableFile(atPath: candidate.path) {
        return candidate
      }
    }
  }

  for path in ["/usr/local/bin/zed", "/opt/homebrew/bin/zed"] {
    if FileManager.default.isExecutableFile(atPath: path) {
      return URL(fileURLWithPath: path)
    }
  }

  return nil
}

// MARK: - Load projects (file-scope, fallback for AppConfig)

private func loadProjects() -> [Project] {
  let candidates = [
    Bundle.main.url(forResource: "projects", withExtension: "json"),
    URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
      .appendingPathComponent("shared/projects.json"),
    URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
      .appendingPathComponent("../shared/projects.json"),
  ].compactMap { $0 }

  for url in candidates {
    if let data = try? Data(contentsOf: url),
      let projects = try? JSONDecoder().decode([Project].self, from: data)
    {
      return projects
    }
  }
  return []
}

// MARK: - ShortcutRecorderField

@MainActor
final class ShortcutRecorderField: NSTextField {
  var recordedModifiers: [String] = []
  var recordedKey: String = ""

  override var acceptsFirstResponder: Bool { true }

  override func keyDown(with event: NSEvent) {
    let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
    let modifiers = carbonFlagsToModifierNames(UInt32(flags.rawValue))

    guard let key = carbonKeyCodeToChar(event.keyCode) else {
      super.keyDown(with: event)
      return
    }
    if modifiers.isEmpty { return }

    recordedModifiers = modifiers
    recordedKey = key
    self.stringValue = readableShortcut(modifiers: modifiers, key: key)
  }

  override func becomeFirstResponder() -> Bool {
    let result = super.becomeFirstResponder()
    if result {
      stringValue = "Press shortcut..."
    }
    return result
  }

  override func resignFirstResponder() -> Bool {
    if recordedKey.isEmpty {
      stringValue = readableShortcut(modifiers: recordedModifiers, key: "m")
    }
    return super.resignFirstResponder()
  }
}

private func readableShortcut(modifiers: [String], key: String) -> String {
  let displayMods = modifiers.map {
    switch $0 {
    case "control": return "⌃"
    case "option": return "⌥"
    case "shift": return "⇧"
    case "command": return "⌘"
    default: return $0
    }
  }
  let displayKey = key == "space" ? "Space" : key.uppercased()
  return displayMods.joined() + displayKey
}

// MARK: - SettingsWindowController

@MainActor
final class SettingsWindowController: NSObject {
  private let window: NSWindow
  private let onSave: (AppConfig) -> Void
  private var projects: [Project]
  private var shortcut: ShortcutConfig
  private var projectList: NSTableView!
  private var detailFields: [String: NSTextField] = [:]
  private var shortcutField: ShortcutRecorderField!

  init(config: AppConfig, onSave: @escaping (AppConfig) -> Void) {
    self.onSave = onSave
    self.projects = config.projects
    self.shortcut = config.shortcut

    window = NSWindow(
      contentRect: NSRect(x: 0, y: 0, width: 620, height: 480),
      styleMask: [.titled, .closable, .resizable],
      backing: .buffered,
      defer: false
    )
    super.init()
    window.title = "Settings"
    window.isReleasedWhenClosed = false
    window.level = .floating
    window.minSize = NSSize(width: 560, height: 400)
    window.center()
    buildUI()
    selectProject(index: 0)
  }

  func showWindow(_ sender: Any?) {
    window.makeKeyAndOrderFront(sender)
  }

  // MARK: Build UI

  private func buildUI() {
    guard let content = window.contentView else { return }

    // Background
    content.wantsLayer = true
    content.layer?.backgroundColor = NSColor.windowBackgroundColor.cgColor

    // Tab view - fills most of the content area
    let tabView = NSTabView(frame: NSRect(x: 20, y: 50, width: 580, height: 410))
    tabView.autoresizingMask = [.width, .height]
    tabView.addTabViewItem(makeGeneralTab())
    tabView.addTabViewItem(makeProjectsTab())
    content.addSubview(tabView)

    // Button bar at bottom
    let btnW: CGFloat = 80
    let btnH: CGFloat = 28
    let saveBtn = NSButton(title: "Save", target: self, action: #selector(save))
    saveBtn.frame = NSRect(x: 580 - btnW, y: 12, width: btnW, height: btnH)
    saveBtn.bezelStyle = .rounded
    saveBtn.keyEquivalent = "\r"

    let cancelBtn = NSButton(title: "Cancel", target: self, action: #selector(cancel))
    cancelBtn.frame = NSRect(x: 580 - btnW * 2 - 8, y: 12, width: btnW, height: btnH)
    cancelBtn.bezelStyle = .rounded
    cancelBtn.keyEquivalent = "\u{1b}"

    content.addSubview(saveBtn)
    content.addSubview(cancelBtn)
  }

  // MARK: General Tab

  private func makeGeneralTab() -> NSTabViewItem {
    let item = NSTabViewItem(identifier: "general")
    item.label = "General"
    let view = NSView(frame: NSRect(x: 0, y: 0, width: 560, height: 380))

    let labelW: CGFloat = 140
    let fieldW: CGFloat = 200
    let x: CGFloat = 20
    let y: CGFloat = 340
    let h: CGFloat = 22

    let shortcutLabel = NSTextField(labelWithString: "Global Shortcut:")
    shortcutLabel.frame = NSRect(x: x, y: y, width: labelW, height: h)
    shortcutLabel.font = .systemFont(ofSize: 13)
    shortcutLabel.alignment = .right
    view.addSubview(shortcutLabel)

    shortcutField = ShortcutRecorderField(frame: NSRect(x: x + labelW + 12, y: y, width: fieldW, height: h))
    shortcutField.isEditable = false
    shortcutField.isBezeled = true
    shortcutField.drawsBackground = true
    shortcutField.font = .systemFont(ofSize: 13)
    shortcutField.stringValue = readableShortcut(modifiers: shortcut.modifiers, key: shortcut.key)
    shortcutField.recordedModifiers = shortcut.modifiers
    shortcutField.recordedKey = shortcut.key
    view.addSubview(shortcutField)

    item.view = view
    return item
  }

  // MARK: Projects Tab

  private func makeProjectsTab() -> NSTabViewItem {
    let item = NSTabViewItem(identifier: "projects")
    item.label = "Projects"
    let view = NSView(frame: NSRect(x: 0, y: 0, width: 560, height: 380))

    // Project list (left sidebar)
    let listW: CGFloat = 180
    let scrollView = NSScrollView(frame: NSRect(x: 8, y: 8, width: listW, height: 364))
    scrollView.hasVerticalScroller = true
    scrollView.borderType = .lineBorder
    scrollView.autoresizingMask = [.height]

    projectList = NSTableView(frame: scrollView.contentView.bounds)
    let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("name"))
    column.title = "Project"
    column.width = listW - 2
    projectList.addTableColumn(column)
    projectList.headerView = nil
    projectList.rowHeight = 26
    projectList.dataSource = self
    projectList.delegate = self
    projectList.target = self
    projectList.usesAlternatingRowBackgroundColors = true
    scrollView.documentView = projectList
    view.addSubview(scrollView)

    // Add/Remove/Browse buttons
    let btnX = listW + 16
    let addBtn = NSButton(title: "+", target: self, action: #selector(addProject))
    addBtn.frame = NSRect(x: btnX, y: 346, width: 28, height: 26)
    addBtn.bezelStyle = .rounded
    view.addSubview(addBtn)

    let removeBtn = NSButton(title: "−", target: self, action: #selector(removeProject))
    removeBtn.frame = NSRect(x: btnX, y: 318, width: 28, height: 26)
    removeBtn.bezelStyle = .rounded
    view.addSubview(removeBtn)

    let browseBtn = NSButton(title: "Browse\u{2026}", target: self, action: #selector(browseProject))
    browseBtn.frame = NSRect(x: btnX, y: 286, width: 70, height: 26)
    browseBtn.bezelStyle = .rounded
    browseBtn.font = .systemFont(ofSize: 11)
    view.addSubview(browseBtn)

    // Detail form
    let detailX = btnX + 38
    let fieldW: CGFloat = 320
    let fieldH: CGFloat = 22
    let labelW: CGFloat = 110
    let rowGap: CGFloat = 28
    let startY: CGFloat = 346
    let fields: [(String, String)] = [
      ("name", "Name"), ("path", "Path"),
      ("openPaths", "Open Paths"), ("aliases", "Aliases"),
      ("tags", "Tags"), ("language", "Language"),
    ]

    for (index, (key, labelText)) in fields.enumerated() {
      let fy = startY - CGFloat(index) * rowGap
      let label = NSTextField(labelWithString: "\(labelText):")
      label.frame = NSRect(x: detailX, y: fy, width: labelW, height: fieldH)
      label.font = .systemFont(ofSize: 12)
      label.alignment = .right
      view.addSubview(label)

      let field = NSTextField(frame: NSRect(x: detailX + labelW + 8, y: fy, width: fieldW - labelW - 8, height: fieldH))
      field.font = .systemFont(ofSize: 13)
      field.target = self
      field.action = #selector(detailFieldChanged)
      detailFields[key] = field
      view.addSubview(field)
    }

    item.view = view
    return item
  }

  // MARK: Actions

  @objc private func save() {
    let mods = shortcutField.recordedModifiers.isEmpty
      ? shortcut.modifiers : shortcutField.recordedModifiers
    let key = shortcutField.recordedKey.isEmpty
      ? shortcut.key : shortcutField.recordedKey

    let config = AppConfig(
      projects: projects,
      shortcut: ShortcutConfig(modifiers: mods, key: key)
    )
    do {
      try config.save()
    } catch {
      let alert = NSAlert()
      alert.messageText = "Failed to save config"
      alert.informativeText = error.localizedDescription
      alert.runModal()
      return
    }
    onSave(config)
    window.close()
  }

  @objc private func cancel() {
    window.close()
  }

  @objc private func addProject() {
    let id = "project-\(UUID().uuidString.prefix(8))"
    let project = Project(
      id: id, name: "New Project", path: "",
      aliases: [], tags: [], language: "Project")
    projects.append(project)
    projectList.reloadData()
    let last = projects.count - 1
    projectList.selectRowIndexes(IndexSet(integer: last), byExtendingSelection: false)
    projectList.scrollRowToVisible(last)
    selectProject(index: last)
  }

  @objc private func removeProject() {
    let selected = projectList.selectedRow
    guard selected >= 0 && selected < projects.count else { return }
    projects.remove(at: selected)
    projectList.reloadData()
    if !projects.isEmpty {
      let next = min(selected, projects.count - 1)
      projectList.selectRowIndexes(IndexSet(integer: next), byExtendingSelection: false)
      selectProject(index: next)
    }
  }

  @objc private func browseProject() {
    let panel = NSOpenPanel()
    panel.canChooseDirectories = true
    panel.canChooseFiles = true
    panel.allowsMultipleSelection = false
    panel.allowedContentTypes = [.folder, .plainText]
    panel.message = "Select a project folder or .code-workspace file"

    guard panel.runModal() == .OK, let url = panel.url else { return }
    let path = url.path
    let name = url.lastPathComponent

    let id = "browse-\(UUID().uuidString.prefix(8))"
    let project = Project(
      id: id, name: name, path: path,
      aliases: [], tags: [], language: "Project")
    projects.append(project)
    projectList.reloadData()
    let last = projects.count - 1
    projectList.selectRowIndexes(IndexSet(integer: last), byExtendingSelection: false)
    projectList.scrollRowToVisible(last)
    selectProject(index: last)
  }

  @objc private func detailFieldChanged() {
    let row = projectList.selectedRow
    guard row >= 0 && row < projects.count else { return }

    let name = detailFields["name"]?.stringValue ?? ""
    let path = detailFields["path"]?.stringValue ?? ""
    let openPathsStr = detailFields["openPaths"]?.stringValue ?? ""
    let openPaths = openPathsStr
      .split(separator: ",").map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
    let aliases = (detailFields["aliases"]?.stringValue ?? "")
      .split(separator: ",").map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
    let tags = (detailFields["tags"]?.stringValue ?? "")
      .split(separator: ",").map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
    let language = detailFields["language"]?.stringValue ?? ""

    let old = projects[row]
    projects[row] = Project(
      id: old.id, name: name, path: path,
      openPaths: openPaths, aliases: aliases,
      tags: tags, language: language, lastOpenedAt: old.lastOpenedAt)
    projectList.reloadData(forRowIndexes: IndexSet(integer: row), columnIndexes: IndexSet(integer: 0))
  }

  private func selectProject(index: Int) {
    guard index >= 0 && index < projects.count else {
      for field in detailFields.values { field.stringValue = "" }
      return
    }
    let p = projects[index]
    detailFields["name"]?.stringValue = p.name
    detailFields["path"]?.stringValue = p.path
    detailFields["openPaths"]?.stringValue = p.openPaths.joined(separator: ", ")
    detailFields["aliases"]?.stringValue = p.aliases.joined(separator: ", ")
    detailFields["tags"]?.stringValue = p.tags.joined(separator: ", ")
    detailFields["language"]?.stringValue = p.language
  }
}

// MARK: - SettingsWindowController + NSTableViewDataSource/Delegate

extension SettingsWindowController: NSTableViewDataSource, NSTableViewDelegate {
  func numberOfRows(in tableView: NSTableView) -> Int {
    projects.count
  }

  func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
    let identifier = NSUserInterfaceItemIdentifier("ProjectCell")
    let textField: NSTextField
    if let existing = tableView.makeView(withIdentifier: identifier, owner: self) as? NSTextField {
      textField = existing
    } else {
      textField = NSTextField(labelWithString: "")
      textField.identifier = identifier
      textField.font = .systemFont(ofSize: 13, weight: .medium)
    }
    guard row < projects.count else { return textField }
    textField.stringValue = projects[row].name
    return textField
  }

  func tableViewSelectionDidChange(_ notification: Notification) {
    selectProject(index: projectList.selectedRow)
  }
}

let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.run()
