import Combine
import RuneCards

struct OpenedCard: Identifiable {
    let id: UInt32
}

@MainActor
final class Cards: ObservableObject {
    @Published private(set) var count: UInt32 = 0
    @Published var opened: OpenedCard?

    init() {
        swiftui_host_listen(SwiftUIHostListener(
            context: Unmanaged.passUnretained(self).toOpaque(),
            cards_changed: { context, count in
                MainActor.assumeIsolated { Cards.from(context).count = count }
            },
            card_opened: { context, number in
                MainActor.assumeIsolated { Cards.from(context).opened = OpenedCard(id: number) }
            }
        ))
    }

    func add() {
        swiftui_host_add_card()
    }

    func remove(_ number: UInt32) {
        swiftui_host_remove_card(number)
    }

    func clear() {
        swiftui_host_clear()
    }

    private nonisolated static func from(_ context: UnsafeMutableRawPointer?) -> Cards {
        Unmanaged<Cards>.fromOpaque(context!).takeUnretainedValue()
    }
}
