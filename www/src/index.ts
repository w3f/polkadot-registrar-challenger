import { AccountStatus, Notification } from "./json";

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

        const action = document.getElementById("specify-action")!.innerHTML;
        const address = (document.getElementById("specify-address")! as HTMLInputElement).value;
        const network = document.getElementById("specify-network")!.innerHTML.toLowerCase();

        if (action == "Request Judgement") {
            const socket = new WebSocket('ws://161.35.213.120/api/account_status');

            socket.addEventListener("open", (_: Event) => {
                socket.send(JSON.stringify({ address: address, chain: network }));

                this.btn_execute_action
                    .innerHTML = "Done!";
            });

            socket.addEventListener("message", (event: Event) => {
                let msg = (event as MessageEvent);
                console.log(msg.data);
                parse_account_status(msg);
            });
        }
    }
}

function parse_account_status(msg: MessageEvent) {
    let table = "";

    const parsed: AccountStatus = JSON.parse(msg.data);
    if (parsed.result_type == "ok") {
        // TODO: Check if 'fields` is empty.

        table + `
            <thead>
                <tr>
                    <th scope="col">#</th>
                    <th scope="col">Type</th>
                    <th scope="col">Challenge</th>
                    <th scope="col">From</th>
                    <th scope="col">To</th>
                    <th scope="col">Status</th>
                </tr>
            </thead>
        `;

        table = '<tbody>';
        for (let field of parsed.message.state.fields) {
            if (field.challenge.challenge_type == "expected_message") {
                table + `
                    <tr>
                        <th scope="row">2</th>
                        <td>${field.value.type}</td>
                        <td>${field.challenge.content.expected.value}</td>
                        <td>${field.value.value}</td>
                        <td>temp</td>
                        <td>${field.challenge.content.expected.is_verified}</td>
                    </tr>
                `;

            } else if (field.challenge.challenge_type == "background_check") {

            }
        }
        table = '</tbody>';


        document.getElementById("verification-overview")!.innerHTML = table;
    } else if (parsed.result_type == "err") {

    } else {
        // Print unexpected error...
    }
}

function display_notification(notifications: Notification[]) {

}

new ActionListerner();
