class ActionListerner {
    constructor() {
        // Handler for choosing network, e.g. "Kusama" or "Polkadot".
        document
            .getElementById("network-options")!
            .addEventListener("click", (e: Event) => {
                document
                    .getElementById("specify-network")!
                    .innerText = (e.target as HTMLAnchorElement).innerText;
            });

        // Handler for choosing action, e.g. "Request Judgement".
        document
            .getElementById("action-options")!
            .addEventListener("click", (e: Event) => {
                document
                    .getElementById("specify-action")!
                    .innerText = (e.target as HTMLAnchorElement).innerText;
            });

        // Handler for executing action and communicating with the backend API.
        document
            .getElementById("execute-action")!
            .addEventListener("click", (_: Event) => this.executeAction());
    }
    executeAction() {
        const network = document.getElementById("specify-network")!;
        const action = document.getElementById("specify-action")!;

    }
}

new ActionListerner();