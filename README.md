# HMM Text-to-Speech

A Rust based Text-to-Speech (TTS) wrapper application built to with [Burn](https://burn.dev/) for model inference and [Iced](https://iced.rs/) for the GUI.

> ⚠️ The app is in early stage, but you can build and test it locally.

<img width="1194" height="761" alt="image" src="https://github.com/user-attachments/assets/cc81907a-6515-44ba-a8b2-385a4d336633" />

## Features
- **Model:** Currently supports only [Kokoro v1.0](https://huggingface.co/hexgrad/Kokoro-82M) (`Kokoro-82M`).
- **Supported Languages:** American and British English voices only (due to limitations of  `loqui-g2p`).
- **GUI:** Built with Iced.
- **Inference:** Burn framework.
