import RuneCards
import RuneView
import SwiftUI

struct ContentView: View {
    @StateObject private var cards = Cards()

    var body: some View {
        TabView {
            NavigationStack {
                RuneView(swiftui_host_view)
                    .ignoresSafeArea()
                    .navigationTitle("Cards")
                    .navigationBarTitleDisplayMode(.inline)
                    .toolbar {
                        ToolbarItem(placement: .topBarLeading) {
                            Button("Clear", role: .destructive) { cards.clear() }
                                .disabled(cards.count == 0)
                        }
                        ToolbarItem(placement: .topBarTrailing) {
                            Button("Add Card", systemImage: "plus") { cards.add() }
                        }
                    }
            }
            .tabItem { Label("Rune", systemImage: "square.stack") }
            .badge(Int(cards.count))

            NavigationStack {
                Form {
                    Section("Shared with the Rune tab") {
                        LabeledContent("Cards", value: "\(cards.count)")
                        Button("Add Card") { cards.add() }
                        Button("Clear All", role: .destructive) { cards.clear() }
                            .disabled(cards.count == 0)
                    }
                }
                .navigationTitle("SwiftUI")
            }
            .tabItem { Label("SwiftUI", systemImage: "swift") }
        }
        .sheet(item: $cards.opened) { card in
            CardSheet(number: card.id) { cards.remove(card.id) }
        }
    }
}

struct CardSheet: View {
    let number: UInt32
    let remove: () -> Void
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            VStack(spacing: 12) {
                Text("Card \(number)")
                    .font(.largeTitle.bold())
                Text("Rune drew the card. SwiftUI shows this sheet.")
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Done") { dismiss() }
                }
                ToolbarItem(placement: .destructiveAction) {
                    Button("Remove", role: .destructive) {
                        remove()
                        dismiss()
                    }
                }
            }
        }
        .presentationDetents([.medium])
    }
}
