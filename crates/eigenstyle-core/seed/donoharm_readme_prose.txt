First, Do No Harm: On Making AI Safe, Secure, and Trustworthy
Authors
Matthew A. Porter, BSc1, Henry Mroch, MD2

See the progress for yourself by clicking the image below!

Abstract
Background
Abstract

The integration of generative artificial intelligence (genAI) into software ecosystems has led to rapid advancements in secure, efficient, and innovative applications. This transformative shift necessitates robust cryptographic safeguards to ensure trust, privacy, and reliability. In our project, "First, Do No Harm," we explore the intersection of post-quantum cryptography (PQC), Unreal Engine, and our custom Neovim implementation, "Diver," enhanced with rose.nvim—a plugin designed to facilitate AI model selection and secure interaction. By leveraging OpenSSL with post-quantum cryptographic algorithms, we ensure that AI communications remain resilient against future cryptographic threats. Our work emphasizes secure key exchange mechanisms and encryption methodologies that protect sensitive AI-driven interactions from adversarial compromise. To demonstrate the application of these technologies in immersive learning and development environments, we have integrated our secure AI framework into Unreal Engine, enabling the deployment of real-time, interactive AI-assisted simulations.
Furthermore, "Diver" extends Neovim with rose.nvim, providing a streamlined interface for locally hosted AI models and secure web-based APIs. This setup allows developers and researchers to interact with AI tools in a controlled, privacy-preserving manner, reinforcing best practices in security and ethical AI deployment. Our approach ensures accessibility while prioritizing safety, equity, and transparency in AI-driven development. Through this initiative, we aim to set new standards for AI safety, security, and trustworthiness, ensuring that generative AI can be harnessed responsibly across diverse domains. By integrating cryptographic resilience, high-performance computing, and developer-friendly tools, our project offers a blueprint for the future of secure AI implementation.

Methods
To address the increasing security concerns in AI-driven applications, we implemented a robust methodology centered on cryptographic integrity, real-time interactivity, and developer accessibility. Our approach prioritizes efficiency without compromising security, ensuring that AI models are deployed safely and effective. For performance optimization, we serve AI models capable of operating on a single consumer grade GPU operating on Arch Linux with CUDA 12.8, TensorRT 10.8, and Pytorch 2.6-cu126. We began by integrating OpenSSL with post-quantum cryptographic (PQC) algorithms, selecting those with demonstrated resilience against both classical and quantum threats. Our cryptographic framework ensures that AI model interactions—whether hosted locally or via secure APIs—remain protected through advanced key exchange protocols and end-to-end encryption techniques. Our custom Neovim implementation, "Diver," incorporates rose.nvim as an AI model interface, providing a seamless experience for researchers and developers. The plugin enables users to select AI models locally or interact with them via secure web-based APIs. All web interactions are secured using PQC-backed TLS encryption to mitigate emerging cryptographic threats. The models operate within sandboxed environments, ensuring secure execution and isolation from the host system. By structuring AI deployment within controlled, cryptographically secure ecosystems, we aim to establish a foundation for AI trustworthiness. Our methodology underscores the importance of privacy, security, and accessibility in AI-driven software development, ensuring that innovations remain both powerful and responsible.

Results

Education

Safety

* Use-Cases

On Increasing SMM Efficiency

TLDR: Using consumer-grade hardware to improve small Open Neural Network Exchange (ONNX) video inference models:

BiRefNet-general-bb_swin_v1_tiny-epoch_232

Isnet-anime

Run with TensorRT 10.8, CUDA 12.8, nvidia-open-dkms on Arch Linux-Zen 6.13-4-1 x86_64

FAQ

Q: How do you mitigate against bias?

TLDR - we do math to make AI ethically useful

A: We delineate between mathematical bias (MB) - a fundamental parameter in neural network equations - and algorithmic/social bias (ASB). While MB is optimized during model training through backpropagation, ASB requires careful consideration of data sources, model architecture, and deployment strategies. We implement attention mechanisms for improved input processing and use legal open-source data and secure web-search APIs to help mitigate ASB.

AI Math at a glance

Forward Propagation Algorithm

y = w_1x_1 + w_2x_2 + ... + w_nx_n + b

Where:

- $y$ represents the model output
- $(x_1, x_2, ..., x_n)$ are input features
- $(w_1, w_2, ..., w_n)$ are feature weights
- $b$ is the bias term
Neural Network Activation

For neural networks, the bias term is incorporated before activation:

z = \sum_{i=1}^{n} w_ix_i + b
a = \sigma(z)

Where:
- $z$ is the weighted sum plus bias
- $a$ is the activation output
- $\sigma$ is the activation function

Attention Mechanism- aka what makes the Transformer (The "T" in ChatGPT) powerful

The Attention mechanism equation is:

\text{Attention}(Q, K, V) = \text{softmax}\left( \frac{QK^T}{\sqrt{d_k}} \right) V

Where:
- $Q$ represents the Query matrix
- $K$ represents the Key matrix
- $V$ represents the Value matrix
- $d_k$ is the dimension of the key vectors
- $\text{softmax}(\cdot)$ normalizes scores to sum to 1

Q: Do I have to buy a Linux computer to use this? I don't have time for that!
A: No. You can run Linux and/or the tools we share alongside your existing operating system:

* Windows users can use Windows Subsystem for Linux WSL
* Mac users can use Homebrew
* The code-base instructions were developed with both beginners and advanced users in mind.

Q: Do you have to get a masters in AI?
A: Not if you don't want to. To get competent enough to get past ChatGPT dependence at least, you just need a computer and a beginning's mindset. Huggingface is a good place to start.

Q: What makes a "small" AI model?
A: AI models ~=10 billion(10B) parameters and below. For comparison, OpenAI's GPT4o contains approximately 200B parameters.

What a Dual-License means

Protection for Vulnerable Populations

The dual licensing aims to address the cybersecurity gap that disproportionately affects underserved populations. As highlighted by recent attacks, low-income residents, seniors, and foreign language speakers face higher-than-average risks of being victims of cyber attacks. By offering both open-source and commercial licensing options, we encourage the development of cybersecurity solutions that can reach these vulnerable groups while also enabling sustainable development and support.

Preventing Malicious Use

The AGPL-3.0 license ensures that any modifications to the software remain open source, preventing bad actors from creating closed-source variants that could be used for exploitation. This is especially crucial given the rising threats to vulnerable communities, including children in educational settings. The attack on Minneapolis Public Schools, which resulted in the leak of 300,000 files and a $1 million ransom demand, highlights the importance of transparency and security.

Addressing Cybersecurity in Critical Sectors

The commercial license option allows for tailored solutions in critical sectors such as healthcare, which has seen significant impacts from cyberattacks. For example, the recent Change Healthcare attack affected millions of Americans and caused widespread disruption for hospitals and other providers. In January 2025, CISA and FDA jointly warned of critical backdoor vulnerabilities in Contec CMS8000 patient monitors, revealing how medical devices could be compromised for unauthorized remote access and patient data manipulation.
Supporting Cybersecurity Awareness

The dual licensing model supports initiatives like the Cybersecurity and Infrastructure Security Agency (CISA) efforts to improve cybersecurity awareness in "target rich" sectors, including K-12 education. By allowing both open-source and commercial use, we aim to facilitate the development of tools that support these critical awareness and protection efforts.

Bridging the Digital Divide

The unfortunate reality is that a number of individuals and organizations have gone into a frenzy in every facet of our daily lives. These unfortunate folks identify themselves with their talk of "10X" returns and building towards Artificial General Intelligence aka "AGI" while offering GPT wrappers. Our dual licensing approach aims to acknowledge this deeply concerning predatory paradigm with clear eyes while still operating to bring the best parts of the open-source community with our services and solutions.

Recent Cybersecurity Attacks

Recent attacks underscore the importance of robust cybersecurity measures:

- The Change Healthcare cyberattack in February 2024 affected millions of Americans and caused significant disruption to healthcare providers.
- The White House and Congress jointly designated October 2024 as Cybersecurity Awareness Month. This designation comes with over 100 actions that align the Federal government and public/private sector partners are taking to help every man, woman, and child to safely navigate the age of AI.

By offering both open-source and commercial licensing options, we strive to create a balance that promotes innovation and accessibility while also providing the necessary resources and flexibility to address the complex cybersecurity challenges faced by vulnerable populations and critical infrastructure sectors.
