class ActionListerner {
    btn_execute_action: HTMLButtonElement;

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

        // Register the action button.
        this.btn_execute_action =
            document
                .getElementById("execute-action")! as HTMLButtonElement;

        // Handler for executing action and communicating with the backend API.
        this.btn_execute_action
            .addEventListener("click", (_: Event) => this.executeAction());
    }
    executeAction() {
        this.btn_execute_action.disabled = true;

        this.btn_execute_action
            .innerHTML = `
                <span class="spinner-border spinner-border-sm" role="status" aria-hidden="true"></span>
                <span class="visually-hidden"></span>
            `;

        const network = document.getElementById("specify-network")!;
        const action = document.getElementById("specify-action")!;

        const socket = new WebSocket('ws://161.35.213.120/api/account_status');

        socket.addEventListener("open", (_: Event) => {
            this.btn_execute_action
                .innerHTML = "Done!";
        });
    }
}

new ActionListerner();
